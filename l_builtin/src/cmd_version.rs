//! Version subcommand - prints build and bash version information

use crate::bash_api::{build_version, dist_version, patch_level, release_status, WORD_LIST};
use crate::bprintln;
use crate::subcmd::{CmdDesc, CmdResult};
use cmdargs_derive::CmdArgs;

// Generated version info (from CMake)
include!(concat!(env!("GENERATED_RUST"), "/version.rs"));

const VERSION_CMD: CmdDesc = CmdDesc::new(
    c"version",
    c"",
    c"\
Print version information for L_builtin and the bash it was compiled against.

Output includes:
  L_builtin version    -- from Cargo.toml
  L_builtin commit     -- git commit of L_builtin source
  Bash version (compile-time) -- version of bash headers used for compilation
  Bash commit (compile-time)  -- git commit of bash source used
  Bash version (runtime)      -- version of bash currently running
",
);

#[derive(CmdArgs)]
struct VersionArgs {
    // no options
}

/// `version`: print build and bash version information
pub unsafe fn version_subcommand(list: *mut WORD_LIST) -> CmdResult {
    VERSION_CMD.enter();
    let _args = VersionArgs::parse(list)?;
    bprintln!(b"L_builtin version: ", L_BUILTIN_VERSION);
    bprintln!(b"L_builtin commit:  ", L_BUILTIN_COMMIT);
    bprintln!(b"Bash version (compile-time): ", BASH_VERSION);
    bprintln!(b"Bash commit (compile-time):  ", BASH_COMMIT);
    bprintln!(
        b"Bash version (runtime):      ",
        unsafe { dist_version },
        b".",
        unsafe { patch_level },
        b"(",
        unsafe { build_version },
        b")-",
        unsafe { release_status }
    );
    // Check array implementation of the running bash
    let alt = unsafe { crate::bash_api::l_array_impl_is_alt() };
    match alt {
        1 => bprintln!(b"Bash array implementation: ALT (dense)"),
        0 => bprintln!(b"Bash array implementation: non-ALT (sparse linked list)"),
        _ => bprintln!(b"Bash array implementation: unknown (error)"),
    };
    Ok(())
}
