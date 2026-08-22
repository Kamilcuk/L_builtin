#include <assert.h>
#include <config.h>
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <stddef.h>
#include <sys/types.h>
#include <dlfcn.h>

#include "bash_api.h"

// My wrappers, that always resolve to external symbols, because of #undef above.
void *l_xmalloc(size_t s) { return xmalloc(s); }
void *l_xrealloc(void *p, size_t s) { return xrealloc(p, s); }
void l_xfree(void *p) { xfree(p); }

// Functions that have to be external to handle #define macros.
ARRAY_ELEMENT *l_array_head(ARRAY *a) { return a->head; }
ARRAY_ELEMENT *l_element_forw(ARRAY_ELEMENT *ae) { return element_forw(ae); }
char *l_element_value(ARRAY_ELEMENT *ae) { return element_value(ae); }
long long l_element_index(ARRAY_ELEMENT *ae) { return (long long)element_index(ae); }
arrayind_t l_array_max_index(ARRAY *a) { return array_max_index(a); }

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

/* Print the long usage message for the currently-executing builtin: the
 * `this_command_name` prefix, the short usage line, and the NULL-terminated
 * `long_doc` array. This is the C counterpart of the former Rust
 * `builtin_usage_long()`
 */
void l_builtin_usage_long(void)
{
  if (this_command_name && *this_command_name) {
    fprintf(stderr, "%s: usage: ", this_command_name);
  }
  if (current_builtin && current_builtin->short_doc) {
    fprintf(stderr, "%s\n", current_builtin->short_doc);
  }
  if (current_builtin && current_builtin->long_doc) {
    fprintf(stderr, "\n");
    for (char **i = (char **)current_builtin->long_doc; *i; i++) {
      fprintf(stderr, "%s\n", *i);
    }
  }
  fflush(stderr);
}

/// Static buffer for the memory, because I do not want to reuse this_command_name.
static char *l_this_command_name_buffer = 0;

/* Enter a subcommand context: append `" prefix"` to `this_command_name` and,
 * when `short_doc` is non-NULL, replace `current_builtin`'s doc pointers so
 * that help/usage for the running subcommand is shown. `long_doc` is wrapped
 * as a NULL-terminated array of C strings (a static two-element array reuses
 * the same storage across calls, which is fine because the docs are replaced
 * again on the next enter and restored by the caller's SubcommandGuard on
 * return). The caller (CmdDesc::enter in Rust) passes NUL-terminated C
 * strings; the leading separator space is added here. The caller is
 * responsible for restoring the previous docs (Rust SubcommandGuard); bash
 * rewinds `this_command_name` after the builtin. */
void l_enter_subcommand(const char *prefix, const char *short_doc, const char *const long_doc[])
{
  {
    assert(prefix && prefix[0]);
    const size_t prefix_len = strlen(prefix);
    const size_t old_len = this_command_name ? strlen(this_command_name) : 0;
    const size_t space_len = old_len > 0 ? 1 : 0;
    char *const buf = l_xrealloc(l_this_command_name_buffer, old_len + space_len + prefix_len + 1);
    assert(buf);
    if (this_command_name != l_this_command_name_buffer) {
      memcpy(buf, this_command_name, old_len);
    }
    if (space_len) {
      buf[old_len] = ' ';
    }
    memcpy(buf + old_len + space_len, prefix, prefix_len + 1);
    this_command_name = l_this_command_name_buffer = buf;
  }
  // Replace current_builtin's doc pointers (only when docs are supplied).
  assert(short_doc);
  assert(long_doc);
  assert(long_doc[0]);
  current_builtin->short_doc = (void *)short_doc;
  current_builtin->long_doc = (void *)long_doc;
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

/* ------------------------------------------------------------------------- */

// copied from variable.c
#define INIT_DYNAMIC_VAR(var, val, gfunc, afunc) \
  do { \
    v = bind_variable(var, (val), 0); \
    v->dynamic_value = gfunc; \
    v->assign_func = afunc; \
  } while (0)

#define INIT_DYNAMIC_ARRAY_VAR(var, gfunc, afunc) \
  do { \
    v = make_new_array_variable(var); \
    v->dynamic_value = gfunc; \
    v->assign_func = afunc; \
  } while (0)

#define INIT_DYNAMIC_ASSOC_VAR(var, gfunc, afunc) \
  do { \
    v = make_new_assoc_variable(var); \
    v->dynamic_value = gfunc; \
    v->assign_func = afunc; \
  } while (0)

/* Initialize a dynamic array variable (l_ prefix version) exposed to Rust.
 *
 * Unlike bash's own static init_dynamic_array_var (which silently returns a
 * pre-existing variable untouched), this attaches the get/set callbacks to the
 * variable *regardless* of where it lives. Crucially, it looks the variable up
 * with find_variable -- which searches every scope, including the locals
 * declared in the caller's function -- so that a `local V; L_builtin shm add
 * V` makes the *local* V a shared dynamic variable rather than creating a
 * separate global that is shadowed by the local (in which case assignments
 * never reach the shared database). If no variable by that name exists, a fresh
 * global array variable is created and the callbacks attached to it. */
SHELL_VAR *l_init_dynamic_array_var(
  const char *name, sh_var_value_func_t *getfunc, sh_var_assign_func_t *setfunc
)
{
  SHELL_VAR *v = find_variable(name);
  if (v == NULL) {
    INIT_DYNAMIC_ARRAY_VAR((char *)name, getfunc, setfunc);
  } else {
    if (array_p(v) == 0) {
      v = convert_var_to_array(v);
    }
    v->dynamic_value = getfunc;
    v->assign_func = setfunc;
  }
  return v;
}

/* Initialize a dynamic associative array variable exposed to Rust.
 * See l_init_dynamic_array_var for the scope/local handling rationale. */
SHELL_VAR *l_init_dynamic_assoc_var(
  const char *name, sh_var_value_func_t *getfunc, sh_var_assign_func_t *setfunc
)
{
  SHELL_VAR *v = find_variable(name);
  if (v == NULL) {
    INIT_DYNAMIC_ASSOC_VAR((char *)name, getfunc, setfunc);
  } else {
    if (assoc_p(v) == 0) {
      v = convert_var_to_assoc(v);
    }
    v->dynamic_value = getfunc;
    v->assign_func = setfunc;
  }
  VSETATTR(v, att_assoc);
  return v;
}

/* Unbind (remove) a shell variable by name; returns 1 if it existed. */
int l_unbind_variable(const char *name) { return unbind_variable((char *)name); }
