#include <config.h>

#include "builtins.h"
#include "shell.h"
#include "L_builtin.h"

const char *const L_builtin_doc[] = {
  "L_lib helper builtins.",
  "",
  "L_builtin [-v VAR] <subcommand> [options] [args]",
  "",
  "Options:",
  "  -v VAR   Capture stdout of the subcommand into shell variable VAR",
  "           (trailing newlines stripped, like $(...))",
  "",
  "Available subcommands:",
  "  accept     Accept a network connection",
  "  capture    Run a command with stdout captured into a variable",
  "  connect    Establish a TCP connection",
  "  core       Core utilities via Rust/uutils",
  "             (run 'L_builtin core --help' to list available)",
  "  eventfd    Create an eventfd counter",
  "  ext        Builtins from bash examples/loadables/ directory",
  "             (run 'L_builtin ext --help' to list available)",
  "  listen     Create a listening TCP socket",
  "  lua        Execute LuaJIT script",
  "  memfd      Create an anonymous memory-backed file",
  "  pipe       Create a pipe",
  "  poll       Wait for file descriptors to become ready",
#if HAVE_PPOLL
  "  ppoll      Wait for FDs and unblock signals atomically",
#endif
  "  recv       Receive bytes from a socket",
  "  send       Send bytes over a socket",
  "  shutdown   Semi-close a network socket",
  "  sigmask    Block or unblock signals",
  "  sigunmask  Unblock signals and run a command",
  "  sleep      High-precision sub-second sleep",
  "  splice     Zero-copy move between two file descriptors",
  "  signalfd   Deliver signals as a file descriptor",
  "  timerfd    Create a timer as a file descriptor",
  "  lseek      Reposition file offset",
  "",
  "Use 'L_builtin <subcommand> --help' for more information.",
  (char *)NULL
};

struct builtin L_builtin_struct = {
  "L_builtin",
  l_entrypoint,
  BUILTIN_ENABLED,
  (void *)L_builtin_doc,
  "L_builtin <subcommand> [options] [args]",
  0
};
