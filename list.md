# Moving bash-agnostic libraries into `llib`

Goal: move self-contained, BASH/OS-agnostic libraries from `l_builtin/src/` into the
`llib` crate (which has no dependency on bash). Only the biggest, most self-contained
libraries are moved; bash-specific code stays.

## Methodology

- Move one library at a time.
- After each move, run `make build` to confirm the code still compiles.
- Prefer adjusting `use` imports over fully-qualified `llib::xxx::yyy` paths in the
  calling code. Write clean code as if it had always lived in `llib`.
- No "wrapper imports" (re-export shims) just to ease the move.
- Add `pub` to symbols that must be visible across the crate boundary.
- `#[cfg(test)]` tests moved into `llib` may be rewritten as standard `cargo test`
  tests (they no longer depend on the bash shim).

## Proposed libraries to move (in order)

1. `intlookup.rs` — const integer-packed lookup table. Pure, no deps. Biggest, fully
   self-contained. Has existing `#[cfg(test)]` tests to convert.
2. `intstr.rs` — `IntStr`/`ToIntStr` integer-to-C-string (only uses `std::os::raw::c_char`).
3. `variadic.rs` — C variadic-call helpers + format-spec compile-time assertion
   (only `c_char`/`CStr`).
4. (`io_common.rs` — already moved; the `Cpnt` type already lives in `llib`.)

## Deliberately NOT moved

- `bash_api.rs`, `cmdargs.rs`, `shared.rs`, `vardb.rs`, `subcmd.rs`, `handles.rs`,
  all `cmd_*.rs` — depend on bash FFI, OS/builtin-specific.
- `nolock.rs` — dead code (declared in `lib.rs` but never used); not a "library used
  by the code".
- `bprint_bytes.rs` — `BDisplay`/`bprint!` macros are tightly coupled with
  `bash_api.rs`, `unittest.rs`, and every command's `beprintln!` usage (macro
  `$crate` paths). Moving it is high-churn for low benefit right now; revisit only
  if the macros can be cleanly decoupled.

## Notes

- `intstr.rs` keeps its `c_char` dependency; that is not a bash dependency, so it is
  fine for `llib` (add `libc` dependency to `llib/Cargo.toml` if needed, or use
  `std::os::raw::c_char`).
- `intlookup.rs`'s `intlookup!` macro currently references `$crate::intlookup::...`;
  when moved it must reference `$crate` (of `llib`) so `l_builtin` calls it as
  `llib::intlookup!` (or `use llib::intlookup`).