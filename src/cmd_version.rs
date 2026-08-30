//! Version subcommand - prints build and bash version information

use crate::bash_api::{build_version, dist_version, patch_level, release_status, WORD_LIST};
use crate::bprintln;
use cmdargs_derive::CmdArgs;
use crate::subcmd::{CmdDesc, CmdResult};

/// Generated version info (from CMake)
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/generated_rust/version.rs"));

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

    bprintln!(b"Bash version (runtime):      ", dist_version, b".", patch_level, b"(", build_version, b")-", release_status);

    Ok(())
}