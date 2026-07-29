#include <config.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "builtins.h"
#include "shell.h"
#include "common.h"
#include "L_builtin.h"

extern char *this_command_name;
extern struct builtin L_builtin_struct;

/* Main help text */
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
  "",
  "Use 'L_builtin <subcommand> -h' for more information.",
  (char *)NULL
};

/* Print help array */
static void print_arr(char **arr)
{
  if (arr) {
    for (int i = 0; arr[i]; i++)
      printf("%s\n", arr[i]);
  }
}

/*
 * Help/usage helpers called from the Rust-side dispatcher (src/dispatch.rs).
 * They live in C because they need access to bash globals (this_command_name,
 * the L_builtin_struct short_doc, and the L_builtin_doc array).
 */

void l_builtin_print_usage(void)
{
  if (this_command_name && *this_command_name)
    fprintf(stderr, "%s: usage: ", this_command_name);
  fprintf(stderr, "%s\n", L_builtin_struct.short_doc);
}

void l_builtin_print_help(void)
{
  print_arr(L_builtin_doc);
}

void l_builtin_unknown_subcommand(const char *name)
{
  fprintf(stderr, "%s: unknown subcommand: %s\n", this_command_name, name);
}

/* `L_builtin_builtin` is provided by the Rust crate (src/dispatch.rs). */

struct builtin L_builtin_struct = {
  "L_builtin",
  L_builtin_builtin,
  BUILTIN_ENABLED,
  L_builtin_doc,
  "L_builtin <subcommand> [options] [args]",
  0
};
