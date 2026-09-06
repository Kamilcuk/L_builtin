use crate::bash_api::{c_char, c_int, l_execute_command_string, WordListView, EX_USAGE, WORD_LIST};
use crate::cmdargs::WordListIterCpnt;
use crate::subcmd::{cint_to_cmd_result, CmdResult};
use cmdargs_derive::CmdArgs;

#[cfg(not(feature = "bash_lt_4_3"))]
fn build_eval_command<'a>(args: impl Iterator<Item = &'a [u8]>) -> Vec<u8> {
    let mut buf = Vec::new();
    for (i, word) in args.enumerate() {
        buf.reserve(word.len() + 2);
        if i > 0 {
            buf.push(b' ');
        }
        buf.push(b'\'');
        for &b in word {
            if b == b'\'' {
                buf.extend_from_slice(b"'\\''");
            } else {
                buf.push(b);
            }
        }
        buf.push(b'\'');
    }
    buf.push(b'\0');
    buf
}

/// # Safety
#[cfg(not(feature = "bash_lt_4_3"))]
const RUN_CMD: crate::subcmd::CmdDesc = crate::subcmd::CmdDesc::new(
    c"run",
    c"<command> [args...]",
    c"\
Run <command> through the shell.
The command is always executed through the shell, so external commands,
shell functions, builtins, and L_builtin subcommands all work uniformly.
Words are single-quoted before being joined, so arguments reach the
command verbatim (no re-splitting or globbing).
Use with -v VAR to capture the command's stdout into a shell variable.
",
);

#[derive(CmdArgs)]
struct RunArgs {
    #[positional]
    command: &'static [u8],
    #[rest]
    args: WordListIterCpnt<'static>,
}

/// # Safety
#[cfg(not(feature = "bash_lt_4_3"))]
pub unsafe fn l_run_subcommand(list: *mut WORD_LIST) -> CmdResult {
    RUN_CMD.enter();
    let args = RunArgs::parse(list)?;
    let cmd = build_eval_command(
        std::iter::once(args.command).chain(args.args.map(|c| unsafe { c.as_bytes() })),
    );
    assert!(!cmd.is_empty());
    cint_to_cmd_result(l_execute_command_string(cmd.as_ptr().cast()))
}
