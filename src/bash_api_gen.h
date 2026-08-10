/* bash_api_gen.h — bindgen input header for bash internal API
 *
 * This header is consumed by *both*:
 *   - bindgen (in build.rs) → generates Rust FFI declarations
 *   - bash_api.c             → compiler checks declarations match definitions
 *
 * It pulls in the Bash headers needed for type resolution and declares every
 * l_* wrapper function defined in bash_api.c so that bindgen can emit the
 * corresponding extern "C" items.
 */

#ifndef L_BUILTIN_BASH_API_GEN_H
#define L_BUILTIN_BASH_API_GEN_H

/* ── POSIX/system types that bash headers rely on (pid_t, etc.) ─────────── */

#include <stddef.h>
#include <stdint.h>
#include <sys/types.h>

/* ── Bash headers (same set compiled by bash_api.c) ─────────────────────── */

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

/* ── Simple allocation wrappers ─────────────────────────────────────────── */

void *l_xmalloc(size_t s);
void *l_xrealloc(void *p, size_t s);
void l_xfree(void *p);
char *l_strdup(const char *str);

/* ── Macro-unwrapping accessors ─────────────────────────────────────────── */

ARRAY_ELEMENT  *l_array_head(ARRAY *a);
ARRAY_ELEMENT  *l_element_forw(ARRAY_ELEMENT *ae);
char           *l_element_value(ARRAY_ELEMENT *ae);
long long       l_element_index(ARRAY_ELEMENT *ae);

int             l_readonly_p(SHELL_VAR *var);
int             l_invisible_p(SHELL_VAR *var);
int             l_array_p(SHELL_VAR *var);
ARRAY          *l_array_cell(SHELL_VAR *var);
int             l_assoc_p(SHELL_VAR *var);
HASH_TABLE     *l_assoc_cell(SHELL_VAR *var);
char           *l_value_cell(SHELL_VAR *var);

int             l_check_unbind_variable(const char *name);

/* ── Compound operations ────────────────────────────────────────────────── */

int  l_assoc_insert(HASH_TABLE *hash, const char *key, const char *value);
char *l_expand_string_to_string_in_quotes(const char *string);

/* ── Conditional: evalstring wrapper (Bash ≥ 4.3) ──────────────────────── */

#if L_BASH_VERSION >= 40300
int  l_execute_command_string(const char *cmd);
#endif

#endif /* L_BUILTIN_BASH_API_GEN_H */