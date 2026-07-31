#include <config.h>

#include "builtins.h"
#include "shell.h"
#include "L_builtin.h"

char *L_builtin_doc[] = {
  "L_lib helper builtins.",
  "",
  "L_builtin <subcommand> [options] [args]",
  "",
  "Available subcommands:",
  "  lseek      Reposition file offset",
  "  poll       Wait for file descriptors to become ready",
#ifdef HAVE_PPOLL
  "  ppoll      Wait for FDs and unblock signals atomically",
#endif
  "  sigmask    Block or unblock signals",
  "  sigunmask  Unblock signals and run a command",
  "  pipe       Create a pipe",
  "  listen     Create a listening TCP socket",
  "  accept     Accept a network connection",
  "  connect    Establish a TCP connection",
  "  shutdown   Semi-close a network socket",
  "  send       Send bytes over a socket",
  "  recv       Receive bytes from a socket",
  "  sleep      High-precision sub-second sleep",
  "  core       Core utilities (ls, stat) via Rust/uutils",
  "  lua        Execute LuaJIT script",
  "  capture    Run a command with stdout captured into a variable",
  "",
  "Use 'L_builtin <subcommand> -h' for more information.",
  (char *)NULL
};

struct builtin L_builtin_struct = {
  "L_builtin",
  L_builtin_builtin,
  BUILTIN_ENABLED,
  L_builtin_doc,
  "L_builtin <subcommand> [options] [args]",
  0
};
