# AGENTS.md — L_builtin

Loadable Bash builtins (C + Rust) compiled into a single `L_builtin.so`,
dispatched as `L_builtin <subcommand> ...` and loaded via `enable -f`.

## Build & verify (do this, not the cargo equivalents)

- `make build` → produces `build/Debug/system/L_builtin.so`.
  The `.so` is **tied to the bash version it was compiled against** (headers
  from that version). `BASH` make var selects the version (`system` default);
  `make release-build` for release.
- `make readme` → regenerates the auto-generated "Subcommand Reference" section
  and its TOC entries in `README.md` (and the TOC bullets) from
  `L_builtin <subcommand> -h`. Requires a built `.so` plus `uv` (the `markdown-it-py`
  library is fetched automatically). The boundaries are marked with
  `<!-- README_GEN_START -->` / `<!-- README_GEN_END -->` (body) and
  `<!-- TOC_GEN_START -->` / `<!-- TOC_GEN_END -->` (TOC). Edit
  `scripts/gen_readme.py` to change generation.
  Requires a built `.so`.
- `make test` → compiles, then runs the **entire** test suite via
  `build/bash/system/bash ./runtests.sh build/Debug/system/L_builtin.so`.
- `make sh` → interactive bash with the builtin already `enable`d (handy for
  manual poking).
- `cargo test` links (test-only `src/test_stubs.rs` provides the bash
  allocator symbols such as `l_xrealloc`/`l_xfree`), but it only runs the Rust
  **unit** tests — it does NOT run the shell suite. Verify the shell suite
  through `make build` + `make test`.

## Testing

- Tests live in `tests/test_*.sh` as `_L_test_*` shell functions, sourced and
  run by `L_unittest_main`. The builtin is loaded as `L_builtin`
  (`enable -f ... L_builtin`).
- `make test` **always runs the full suite** — passing a name to
  `make test ARGS=...` does NOT filter (extra args are treated as additional
  test names, not a substring filter). To run a single test, start a bash with
  the builtin enabled, `source tests/test_shm.sh` + `source build/L_lib.sh -s`,
  then call the `_L_test_*` function directly.
- The test harness `L_lib.sh` (pinned v2.0.4) is **auto-downloaded** to
  `build/L_lib.sh` on first `make test` (needs network). It provides
  `L_unittest_*`, `L_with_process_into`, etc.
- Each `L_` harness function documents itself: run `L_lib.sh L_<func> -h`
  (e.g. `L_lib.sh L_finally -h`) for its usage and semantics. `L_finally`
  registers cleanup actions that run automatically at the end of a test
  (use it instead of manual teardown).
- Fork-based sharing tests use `L_with_process_into pid bg "$tmpf"`; the child
  inherits bound bash variables, and `bg` must redirect its output to `"$1"`
  (the tmpfile) itself — the harness does not capture stdout to the file.
- Don't assert on `info`/`ls` **printed text** (it mirrors bash's array
  representation, which changes across bash versions). Verify the actual
  variable values via a forked read-back instead.

## Architecture

- `src/entrypoint.rs` is the C entry point; it dispatches to per-feature
  modules (`cmd_shm.rs`, `cmd_semaphore.rs`, `cmd_mutex.rs`, `cmd_barrier.rs`,
  `cmd_lua.rs`, `cmd_*.rs`). Each module is `pub(crate) mod` in `src/lib.rs`.
- Subcommand groups follow one pattern: a `X_CMD: CmdDesc`, an
  `X_SUBCOMMANDS: &[(&str, SubcommandFn)]`, and
  `X_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, N>` built by
  `crate::intlookup!(&X_SUBCOMMANDS)`. **`N` must equal the entry count** — use
  the `U64` module for ≥5 entries; a mismatch is a compile error.
- Option parsing uses the `#[derive(CmdArgs)]` system (cmdargs.rs /
  cmdargs-derive). Notes:
  - option with a value: `#[opt('c')] name: Option<T>`
  - boolean flag: `#[flag('c')] name: bool`
  - required positional: `#[positional] name: T`
  - optional positional: `#[optional] name: Option<T>`
  - remaining words as a view: `#[rest] name: WordListView<'static>`
  - every subcommand must call `CMD.enter()` then `XxxArgs::parse(list)`;
    `parse` does NOT call `enter()` itself.

## The `shm` subcommand (non-obvious semantics)

Bash array variables (indexed or associative, `-A`) shared across processes
through a `rkyv` database. Backing is chosen by flags on every subcommand:

- `-s NAME` → POSIX shared memory (`shm_open`); shared by **name across
  unrelated processes**.
- `-n NAME` → a **named anonymous memfd**; shared only within this process tree
  (forked children).
- `-f PATH` → a regular file.
- no flag → memfd named `DEFAULT`.

Subcommands: `add [-A]`, `rm` (whole database), `unbind VAR...`
(registry-only unbind, leaves db data), `info`, `ls`.

**Gotcha:** `memfd_create` does **not** dedupe by name. So two `add -n NAME A`
and `add -n NAME B` calls must reuse the same in-process memfd via the variable
REGISTRY (see `get_db_loc` in `cmd_shm.rs`); `-s`/`-f` reuse at the OS level.
Do not re-`open` a memfd by name expecting the same object.

## Generated / tooling

- `generated_rust/bash_api_gen.rs` is produced by bindgen (cargo prebuild) —
  never hand-edit; rebuild to regenerate.
- `compile_commands.json` is a symlink into the build dir for clang tooling
  (`make tidy`/`cppcheck`).
- `make format` runs `clang-format -i src/*.c src/*.h` + `cargo fix`
  (Rust). It does **not** run rustfmt; use `cargo fmt` for Rust formatting.
  `make rustchecks` adds `cargo fmt --check` + clippy + `cargo test`
  (the `cargo test` step fails to link here — see above).

## Public API

The **only** public API for `L_builtin` is the **CLI** — the set of `L_builtin <subcommand>`
invocations as documented in `README.md`. All Rust types and functions in `src/`
(`cmd_shm.rs`, `vardb.rs`, `cmd_mutex.rs`, etc.) are `pub(crate)` and exist solely to
implement that CLI. There are no external Rust consumers and no public library API.

This means **internal refactoring, restructuring, or simplification of module boundaries
within `src/` is unconstrained** as long as:
- The CLI behavior (subcommand names, flags, output, exit codes) is unchanged.
- `make test` still passes (the shell test suite is the source of truth for behavior).
- The module wiring in `src/lib.rs` (`pub(crate) mod ...`) is kept consistent with the
  file layout.

In particular, types like `DbPath`, `DbLoc`, `LockedDatabase`, `VarData`, `DatabaseRepr`
in `vardb.rs` and the functions imported by `cmd_shm.rs` are internal implementation details.
They may be freely changed, inlined, or reorganized to reduce complexity — e.g. collapsing
the `Backend` enum and `FlockDb`/`MemfdDb` structs into a single `LockedDatabase` if the
pshared-rwlock approach suffices, or removing the `Backend` indirection if only one
locking strategy remains.

## CI & known issues

- `.github/workflows/ci.yml` only **builds** the `.so` via Docker
  (`make dockerfile`, release). It does **not** run `make test` — a green CI
  does not imply tests pass.

