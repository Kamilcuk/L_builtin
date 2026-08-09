#include <config.h>
#include <errno.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
#include <sys/types.h>
#include <dlfcn.h>

#include "bash_api.h"
// bash_api_gen.h declares every l_* wrapper defined below, so the compiler
// checks each declaration against its definition. It also pulls in all the
// bash headers (shell.h, variables.h, array.h, assoc.h, command.h, general.h,
// externs.h, subst.h, execute_cmd.h, unwind_prot.h, builtins/common.h,
// trap.h) exactly once — shell.h has no include guard, so do NOT include those
// headers directly here or redefinition errors result.
#include "bash_api_gen.h"

// For rust I require external symbols, so no macros.
// These x* are defined for both Bash internal and non-internal allopcation paths.
#ifdef xmalloc
#undef xmalloc
#endif
#ifdef xrealloc
#undef xrealloc
#endif
#ifdef xfree
#undef xfree
#endif
extern void *xmalloc(size_t);
extern void *xrealloc(void *, size_t);
extern void xfree(void *);

// My wrappers, that always resolve to external symbols, because of #undef above.
void *l_xmalloc(size_t s) { return xmalloc(s); }
void *l_xrealloc(void *p, size_t s) { return xrealloc(p, s); }
void l_xfree(void *p) { xfree(p); }

// Functions that have to be external to handle #define macros.
ARRAY_ELEMENT *l_array_head(ARRAY *a) { return a->head; }
ARRAY_ELEMENT *l_element_forw(ARRAY_ELEMENT *ae) { return element_forw(ae); }
char *l_element_value(ARRAY_ELEMENT *ae) { return element_value(ae); }
long long l_element_index(ARRAY_ELEMENT *ae) { return (long long)element_index(ae); }
int l_readonly_p(SHELL_VAR *var) { return readonly_p(var); }
int l_invisible_p(SHELL_VAR *var) { return invisible_p(var); }
int l_array_p(SHELL_VAR *var) { return array_p(var); }
ARRAY *l_array_cell(SHELL_VAR *var) { return array_cell(var); }
int l_assoc_p(SHELL_VAR *var) { return assoc_p(var); }
HASH_TABLE *l_assoc_cell(SHELL_VAR *var) { return assoc_cell(var); }
char *l_value_cell(SHELL_VAR *var) { return value_cell(var); }

// Check_unbind_variable is missing in older bash versions.
int l_check_unbind_variable(const char *name)
{
  SHELL_VAR *v = find_variable(name);
  if (v && readonly_p(v)) {
    return 10000;
  } else if (v && non_unsettable_p(v)) {
    return 10001;
  }
  return unbind_variable(name);
}

// Just strdup that uses xmalloc.
char *l_strdup(const char *str)
{
  if (!str)
    return NULL;
  const size_t len = strlen(str) + 1;
  char *const ret = xmalloc(len);
  memcpy(ret, str, len);
  return ret;
}

/*
 * Insert into an associative array. assoc_insert takes ownership of the key
 * (frees it when overwriting an existing entry) but copies the value; give it
 * an owned copy of the key so callers can pass borrowed pointers for both.
 */
int l_assoc_insert(HASH_TABLE *hash, const char *key, const char *value)
{
  return assoc_insert(hash, l_strdup(key), (char *)value);
}

/* Expand a string with double-quote semantics and return an owned string
 * (caller must free with free()) */
char *l_expand_string_to_string_in_quotes(const char *string)
{
  return expand_string_to_string((char *)string, Q_DOUBLE_QUOTES);
}

/* ------------------------------------------------------------------------- */

// no evalstring before 4.3, lets just ignore before that.
#if L_BASH_VERSION >= 40300

// Needed to cast set_error_trap into void (*)(void*);
// Newer bash version have uv_set_error_trap.
static inline void l_set_error_trap(void *p) { set_error_trap(p); }

/*
 * Eval
 * SEVAL_NOFREE keeps parse_and_execute from taking ownership of our buffer;
 * SEVAL_NOHIST keeps the command out of history.
 */
int l_execute_command_string(const char *cmd)
{
  begin_unwind_frame("L_capture");
  extern int exit_immediately_on_error;
  unwind_protect_int(exit_immediately_on_error);
  exit_immediately_on_error = 0;
  extern int builtin_ignoring_errexit;
  unwind_protect_int(builtin_ignoring_errexit);
  builtin_ignoring_errexit = 1;
  // save, disable and restore the ERR trap
  char *error_trap = TRAP_STRING(ERROR_TRAP);
  if (error_trap) {
    error_trap = l_strdup(error_trap);
    add_unwind_protect(l_xfree, error_trap);
    add_unwind_protect(l_set_error_trap, error_trap);
    restore_default_signal(ERROR_TRAP);
  }
  const int ret = evalstring((char *)cmd, "L_builtin capture", SEVAL_NOHIST | SEVAL_NOFREE);
  run_unwind_frame("L_capture");
  extern int errexit_flag;
  exit_immediately_on_error = builtin_ignoring_errexit ? 0 : errexit_flag;
  return ret;
}

#endif
