# Which config.h / `./configure` options break `l_builtin` ABI

Relevant only when `l_builtin`'s bash headers differ from the bash it is loaded
into. Machine/compiler data-model features (pointer width, `SIZEOF_*`,
`HAVE_LONG_DOUBLE`, 32/64-bit) are **ruled out** — they are identical for every
build on a given machine. What remains is *bash configuration* changing the
internal representation of structures `l_builtin` uses.

## Breaks it (representation change + code path `l_builtin` traverses)

### 1. `--enable-alt-array-implementation` (`ALT_ARRAY_IMPLEMENTATION`) — the one real hazard

Configured by `configure.ac:219`, `AC_DEFINE` at `configure.ac:371`. This does
not merely toggle a flag — it swaps the entire array representation in
`array.h:30-49` **and** swaps `array2.o` for `array.o` at link time.

Default (sparse linked list):

```c
typedef struct array {
    arrayind_t max_index;          /* 8 */
    arrayind_t num_elements;       /* 8 */
    struct array_element *head;    /* 8 */
    struct array_element *lastref; /* 8 */
} ARRAY;                           /* size 32 */

typedef struct array_element {
    arrayind_t ind;                /* 8 */
    char *value;                   /* 8 */
    struct array_element *next, *prev;  /* 8 + 8 */
} ARRAY_ELEMENT;                   /* size 32 */
```

`ALT_ARRAY_IMPLEMENTATION` (dense array):

```c
typedef struct array {
    arrayind_t max_index;          /* 8 */
    arrayind_t num_elements;       /* 8 */
    arrayind_t first_index;        /* 8 */
    arrayind_t alloc_size;         /* 8 */
    struct array_element **elements;  /* 8 */
} ARRAY;                           /* size 40 */

typedef struct array_element {
    arrayind_t ind;                /* 8 */
    char *value;                   /* 8 */
} ARRAY_ELEMENT;                   /* size 16 (no next/prev) */
```

`l_builtin` **does** dereference these fields directly, so this option changes a
code path it actually walks:

- `bash_api.c:19` — `l_array_head(ARRAY *a) { return a->head; }`
- `bash_api.c:20` — `l_element_forw(ae) { return element_forw(ae); }` (macro →
  `ae->next` default, function call under ALT)
- `bash_api.c:22` — `l_element_index(ae) { return element_index(ae); }` (`ae->ind`)

These wrappers are called from Rust to iterate arrays:
`bash_api.rs:323-350` and `cmd_lua.rs:360-372`. Under ALT they fail to compile
(`a->head` does not exist), and mixing configs reads the wrong memory.

The macro also changes exported helper signatures:

- `array_slice(ARRAY *, ARRAY_ELEMENT *, ARRAY_ELEMENT *)` (default)
  vs `array_slice(ARRAY *, arrayind_t, arrayind_t)` (ALT) — `array.h:66-70`
- `array_shift(ARRAY *, int, int)` returning `ARRAY_ELEMENT *` (default)
  vs returning `ARRAY_ELEMENT **` (ALT) — `array.h:74-78`

### 2. `--enable-multibyte` (`NO_MULTIBYTE_SUPPORT` → `HANDLE_MULTIBYTE`)

Derived in `config-bot.h:136-164` from `NO_MULTIBYTE_SUPPORT`
(`config.h:152`) plus `HAVE_WCHAR_H`/`HAVE_WCTYPE_H`/`HAVE_MBSTATE_T`. Adds or
removes multibyte state members guarded by `#if defined(HANDLE_MULTIBYTE)` in
shared structs, e.g. `shell.h:234-237`. `l_builtin` does not currently deref
those structs, but it is a layout toggle on shared types.

## Theoretically changes layout, but not on a path `l_builtin` triggers

### 3. `--enable-select`, `--enable-dparen-arithmetic`, `--enable-cond-command`, `--enable-arith-for-command`

Gate extra members in the `COMMAND.value` union (`command.h:211-222`). Each is a
pointer-width union member, so toggling any changes `sizeof(COMMAND)`.

**However `l_builtin`'s only `COMMAND` dereference is not layout-sensitive.**
`sig.c:285-288`:

```c
COMMAND *cmd = L_make_bare_simple_command();
cmd->value.Simple->words = copy_word_list(list);
int result = execute_command(cmd);
dispose_command(cmd);
```

- `value.Simple` is the **unconditional** first union member (`command.h:208`),
  so its offset is fixed regardless of which `#if`-gated members
  (`Select`/`Arith`/`Cond`/`ArithFor`) are present — those are appended *after*
  `Simple` and are pointer-sized, so they do not shift it.
- `SIMPLE_COM` (`command.h:337`) is config-stable:
  `{int flags; int line; WORD_LIST *words; REDIRECT *redirects;}`, no `#if`
  gating, so `words` is at a fixed offset.

The deref path (`value.Simple->words`) never crosses a `#if` boundary. **No
effect.**

### 4. `--enable-array-variables` (`ARRAY_VARS`)

Adds `ARRAY *pipestatus` to `sh_parser_state_t` (`shell.h:208`), shifting offsets
after it. `l_builtin` does not embed or deref that struct. No effect.

## Irrelevant (behavior only, no layout/representation change)

`--enable-alias`, `--enable-bang-history`, `--enable-history`, `--enable-readline`,
`--enable-progcomp`, `--enable-coprocesses`, `--enable-debugger`,
`--enable-casemod-*`, `--enable-extended-glob`, `--enable-command-timing`,
`--enable-net-redirections`, `--enable-process-substitution`, `--enable-restricted`,
`--enable-job-control`, `--enable-directory-stack`, `--enable-xpg-echo-default`,
`--enable-strict-posix-default`, `--enable-translatable-strings`,
`--enable-function-import`, `--enable-brace-expansion`, `--enable-mem-scramble`,
`--with-bash-malloc` (allocator only: no struct layout, `l_builtin` already routes
through `l_xmalloc`/`l_xrealloc`/`l_xfree`).

## Bottom line

Only `--enable-alt-array-implementation` (and to a lesser degree
`--enable-multibyte`) can break `l_builtin`, because those are the only options
that both change an internal struct's representation **and** sit on a code path
`l_builtin` actually dereferences. The rest are either gated on structs `l_builtin`
never touches directly, or purely behavioral.

## Recommendation

- Pin the bash used to generate `bash_api_gen.rs` and the bash the `.so` loads
  into to the same flags — especially the same value of
  `--enable-alt-array-implementation` (default off) and `--enable-multibyte`.
- Because `ARRAY` size is baked in as `OpaqueArray<u64,4>` (32 bytes), a build
  with ALT enabled (40 bytes) would leave the opaque size wrong and must be
  regenerated.

## Runtime detection of array implementation

As a convenience, `l_builtin` provides a function to query the array
implementation of the bash into which it is loaded:

- `int l_array_impl_is_alt(void)` — returns 1 if the running bash uses the
  `ALT_ARRAY_IMPLEMENTATION` (dense array), 0 if it uses the default
  sparse linked‑list array, and -1 on failure (e.g. out of memory).

The function works by calling `array_create()` and inspecting the first
two machine‑words of the resulting `ARRAY *`:
  * Non‑ALT: offset 16 holds a non‑NULL pointer to the dummy head element.
  * ALT:     offset 16 holds the integer `-1` (the `first_index` field).

This allows subcommands or the loader to adapt behavior if needed, although
the primary defense is to build and load with matching `--enable-alt-array-implementation`
settings.

The `version` subcommand makes use of this function to display the array
implementation of the running bash.