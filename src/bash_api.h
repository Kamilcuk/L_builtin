/* bash_api.h - bindgen input header for bash internal API
 *
 * This header is consumed by *both*:
 *   - bindgen (in build.rs) -> generates Rust FFI declarations
 *   - bash_api.c             -> compiler checks declarations match definitions
 *
 * It pulls in the Bash headers needed for type resolution and declares every
 * l_* wrapper function defined in bash_api.c so that bindgen can emit the
 * corresponding extern "C" items.
 */

#ifndef L_BUILTIN_BASH_API_H_
#define L_BUILTIN_BASH_API_H_

/* -- POSIX/system types that bash headers rely on (pid_t, etc.) ----------- */

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

/* -- Bash headers (same set compiled by bash_api.c) ----------------------- */

#include <shell.h>
#include <builtins.h>
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
#include <trap.h>
#include <bashgetopt.h>

/* -- Simple allocation wrappers ------------------------------------------- */

void *l_xmalloc(size_t s);
void *l_xrealloc(void *p, size_t s);
void l_xfree(void *p);
char *l_strdup(const char *str);

#ifdef malloc
#undef malloc
#endif
#define malloc l_xmalloc
#ifdef realloc
#undef realloc
#endif
#define realloc l_xrealloc
#ifdef free
#undef free
#endif
#define free l_xfree

/* -- Macro-unwrapping accessors ------------------------------------------- */

ARRAY_ELEMENT *l_array_head(ARRAY *a);
ARRAY_ELEMENT *l_element_forw(ARRAY_ELEMENT *ae);
char *l_element_value(ARRAY_ELEMENT *ae);
long long l_element_index(ARRAY_ELEMENT *ae);
arrayind_t l_array_max_index(ARRAY *a);

int l_readonly_p(SHELL_VAR *var);
int l_invisible_p(SHELL_VAR *var);
int l_array_p(SHELL_VAR *var);
ARRAY *l_array_cell(SHELL_VAR *var);
int l_assoc_p(SHELL_VAR *var);
HASH_TABLE *l_assoc_cell(SHELL_VAR *var);
char *l_value_cell(SHELL_VAR *var);

int l_check_unbind_variable(const char *name);

/* -- Compound operations -------------------------------------------------- */

int l_assoc_insert(HASH_TABLE *hash, const char *key, const char *value);
char *l_expand_string_to_string_in_quotes(const char *string);

/* -- Subcommand context helpers (used by the Rust dispatcher) ------------ */

void l_builtin_usage_long(void);
void l_enter_subcommand(const char *prefix, const char *short_doc, const char *const long_doc[]);

/* -- Conditional: evalstring wrapper (Bash >= 4.3) ------------------------ */

#if L_BASH_VERSION >= 40300
int l_execute_command_string(const char *cmd);
#endif

/* ------------------------------------------------------------------------- */

SHELL_VAR *l_init_dynamic_array_var(
  const char *name, sh_var_value_func_t *getfunc, sh_var_assign_func_t *setfunc, int attrs
);

/* Initialize a dynamic associative array variable exposed to Rust.
 * Creates an associative array variable and attaches the dynamic value /
 * assignment callbacks. */
SHELL_VAR *l_init_dynamic_assoc_var(
  const char *name, sh_var_value_func_t *getfunc, sh_var_assign_func_t *setfunc, int attrs
);

/* Unbind (remove) a shell variable by name; returns 1 if it existed. */
int l_unbind_variable(const char *name);

/* ------------------------------------------------------------------------- */

/*
 * Flag lookup tables for the fcntl subcommand (defined in cmd_fcntl.c).
 * Each entry maps a human-readable flag name to its numeric constant.
 * The arrays are sentinel-terminated ({NULL, 0}) so Rust can iterate
 * without a compile-time length.
 */
typedef struct {
    const char *name;
    int flag;
} l_flag_entry_t;

extern const l_flag_entry_t l_open_flags[];
extern const l_flag_entry_t l_fd_flags[];

#endif /* L_BUILTIN_BASH_API_H_ */
