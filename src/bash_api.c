#include <config.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
#include <sys/types.h>
#include <shell.h>
#include <variables.h>
#include <array.h>
#include <assoc.h>
#include <command.h>
#include <general.h>
#include <externs.h>
#include <subst.h>
#include <execute_cmd.h>
#include <unwind_prot.h>
#include <builtins/common.h>
#include "trap.h"

/* Declared in bash's flags.h and trap.c, which are not installed with the
 * public bash headers. Both are exported by the bash binary (-rdynamic). */
extern int errexit_flag;
extern int exit_immediately_on_error;
extern void uw_set_error_trap(void *);

/* Only bridge what Rust CANNOT call directly without knowing struct layouts */

ARRAY_ELEMENT *l_array_head(ARRAY *a) { return a->head; }

ARRAY_ELEMENT *l_element_forw(ARRAY_ELEMENT *ae) { return element_forw(ae); }

char *l_element_value(ARRAY_ELEMENT *ae) { return element_value(ae); }

/* Bash array indices are signed and may be sparse, so the index has to be
 * read alongside the value to reproduce an array faithfully. */
long long l_element_index(ARRAY_ELEMENT *ae) { return (long long)element_index(ae); }

WORD_LIST *l_word_list_next(WORD_LIST *list) { return list->next; }

WORD_DESC *l_word_list_word(WORD_LIST *list) { return list->word; }

char *l_word_desc_string(WORD_DESC *word) { return word->word; }

/* Wrapper for readonly_p macro - checks if variable has readonly attribute */
int l_readonly_p(SHELL_VAR *var) { return readonly_p(var); }

/* Wrapper for invisible_p macro - checks if variable has invisible attribute */
int l_invisible_p(SHELL_VAR *var) { return invisible_p(var); }

/* Wrapper for array_p macro - checks if variable is an array */
int l_array_p(SHELL_VAR *var) { return array_p(var); }

/* Wrapper for array_cell macro - gets array pointer from variable */
ARRAY *l_array_cell(SHELL_VAR *var) { return array_cell(var); }

/* Wrapper for assoc_p macro - checks if variable is an associative array */
int l_assoc_p(SHELL_VAR *var) { return assoc_p(var); }

/* Wrapper for assoc_cell macro - gets hash table pointer from variable */
HASH_TABLE *l_assoc_cell(SHELL_VAR *var) { return assoc_cell(var); }

/*
 * Insert into an associative array. assoc_insert takes ownership of the key
 * (frees it when overwriting an existing entry) but copies the value; give it
 * an owned copy of the key so callers can pass borrowed pointers for both.
 */
int l_assoc_insert(HASH_TABLE *hash, const char *key, const char *value)
{
  size_t klen = strlen(key) + 1;
  char *kcopy = (char *)xmalloc(klen);
  memcpy(kcopy, key, klen);
  return assoc_insert(hash, kcopy, (char *)value);
}

/* Wrapper for value_cell macro - gets value pointer from variable */
char *l_value_cell(SHELL_VAR *var) { return value_cell(var); }

/* Expand a string with double-quote semantics and return an owned string
 * (caller must free with free()) */
char *l_expand_string_to_string_in_quotes(const char *string)
{
  return expand_string_to_string((char *)string, Q_DOUBLE_QUOTES);
}

/*
 * Execute a word list as a single shell command and return its exit status.
 *
 * Each word is single-quoted before being joined, so the words reach the
 * command exactly as given: no re-splitting, no globbing, no second round of
 * expansion. This makes `L_builtin capture VAR cmd arg...` behave like running
 * `cmd arg...` directly, and works uniformly for external commands, shell
 * functions, builtins, and aliases-free command lines.
 *
 * SEVAL_NOFREE keeps parse_and_execute from taking ownership of our buffer;
 * SEVAL_NOHIST keeps the command out of history.
 */
int l_execute_word_list(WORD_LIST *list)
{
  size_t len = 0;
  char *cmd, *p, *error_trap;
  int ret;
  WORD_LIST *l;

  if (list == NULL)
    return EXECUTION_FAILURE;

  /* First pass: size the buffer from the quoted forms. */
  for (l = list; l; l = l->next) {
    char *q = sh_single_quote(l->word->word ? l->word->word : "");
    if (q == NULL)
      return EXECUTION_FAILURE;
    len += strlen(q) + 1; /* separator or NUL */
    free(q);
  }

  cmd = (char *)malloc(len + 1);
  if (cmd == NULL)
    return EXECUTION_FAILURE;

  /* Second pass: join the quoted words with single spaces. */
  p = cmd;
  for (l = list; l; l = l->next) {
    char *q = sh_single_quote(l->word->word ? l->word->word : "");
    if (q == NULL) {
      free(cmd);
      return EXECUTION_FAILURE;
    }
    size_t n = strlen(q);
    memcpy(p, q, n);
    p += n;
    free(q);
    if (l->next)
      *p++ = ' ';
  }
  *p = '\0';

  /*
   * Suppress `set -e` and the ERR trap for the nested execution, the same way
   * bash does for `eval` in an ignore-return context (execute_cmd.c).
   *
   * Without this a failing captured command would run the ERR trap and
   * longjmp out of parse_and_execute from *inside* the capture: fd 1 would
   * stay redirected to the memfd and the caller's `|| ...` guard would never
   * run. Bash cannot set CMD_IGNORE_RETURN for us here, because
   * parse_and_execute only honours it for the eval and source builtins.
   *
   * Errexit and the ERR trap still apply to `L_builtin capture` itself, which
   * returns the captured command's status -- so it behaves like any ordinary
   * command, not like an eval.
   *
   * The unwind frame restores all three pieces of state even if the captured
   * command longjmps (`exit`, a syntax error, an interrupt).
   */
  begin_unwind_frame("L_capture");
  add_unwind_protect(xfree, cmd);
  unwind_protect_int(exit_immediately_on_error);
  unwind_protect_int(builtin_ignoring_errexit);
  error_trap = TRAP_STRING(ERROR_TRAP);
  if (error_trap) {
    /* Bounded copy instead of savestring(), whose strcpy() trips
     * clang-analyzer-security.insecureAPI.strcpy. */
    size_t tlen = strlen(error_trap) + 1;
    char *tcopy = (char *)xmalloc(tlen);
    memcpy(tcopy, error_trap, tlen);
    error_trap = tcopy;
    add_unwind_protect(xfree, error_trap);
    add_unwind_protect(uw_set_error_trap, error_trap);
    restore_default_signal(ERROR_TRAP);
  }
  exit_immediately_on_error = 0;
  builtin_ignoring_errexit = 1;

  ret = parse_and_execute(cmd, "L_builtin capture", SEVAL_NOFREE | SEVAL_NOHIST);

  /* Restores the trap, both flags, and frees cmd. */
  run_unwind_frame("L_capture");
  exit_immediately_on_error = builtin_ignoring_errexit ? 0 : errexit_flag;

  return ret;
}
