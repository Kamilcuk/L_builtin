#include <config.h>
#include <errno.h>
#include <signal.h>
#include <stdio.h>
#include <string.h>

#include "l_builtins.h"

/* Missing extern declarations from Bash headers */
extern sigset_t top_level_mask;
extern int pending_traps[NSIG];
extern int line_number;

static void restore_process_sigmask(void *arg)
{
  sigset_t *mask = (sigset_t *)arg;
  sigprocmask(SIG_SETMASK, mask, NULL);
}

static int parse_sigspec(WORD_LIST *list, sigset_t *set)
{
  int sig;
  while (list) {
    if (strcasecmp(list->word->word, "all") == 0) {
      sigfillset(set);
    } else {
      sig = decode_signal(list->word->word, DSIG_NOCASE | DSIG_SIGPREFIX);
      if (sig == NO_SIG) {
        sh_invalidsig(list->word->word);
        return -1;
      }
      sigaddset(set, sig);
    }
    list = list->next;
  }
  return 0;
}

static const char *const sigmask_doc[] = {
  "Block or unblock signals.",
  "",
  "L_builtin sigmask [-s sigspec] [-u sigspec] [sigspec ...]",
  "",
  "Block or unblock signals in the shell process. Without options, it",
  "prints the current signal mask. -s blocks, -u unblocks.",
  "Each of -s and -u takes a single sigspec (e.g. -s INT, then -u TERM",
  "in two flags). For multiple signals, repeat the flag (-s INT -s TERM)",
  "use 'ALL' (case-insensitive), or list remaining signals as positional",
  "args (positional args are always BLOCKED, never unblocked).",
  "",
"Examples:",
  "  # Block INT/TERM for the duration of a critical loop, unblocking them",
  "  # only around the sleep so the user can cancel with Ctrl-C.",
  "  trap 'cancel=1' INT TERM",
  "  L_builtin sigmask INT TERM",
  "  while ! cancel; do",
  "    echo 'critical step'",
  "    L_builtin sigunmask -s INT sleep 1",
  "  done",
  "",
  "  # Once a signal is unmasked with -u, it stays unmasked for every",
  "  # subsequent command in this shell (changes persist via top_level_mask).",
  "  trap 'echo USR1' USR1",
  "  L_builtin sigmask -s USR1",
  "  L_builtin sigmask          # shows SIGUSR1 is blocked",
  "  L_builtin sigmask -u USR1",
  "  L_builtin sigmask          # shows SIGUSR1 is no longer listed",
  "  L_raise -USR1              # trap fires immediately",
  "",
  "  # Mixed flags + positional args: -u unblocks INT, then TERM (positional)",
  "  # is blocked. This is NOT equivalent to 'unblock both INT and TERM'.",
  "  L_builtin sigmask -s INT TERM USR1",
  "  L_builtin sigmask -u INT TERM   # unblock INT, block TERM again",
  "",
  "Exit Status:",
  "Returns success unless an invalid signal is provided or a system error ",
  "occurs.",
  (char *)NULL
};

int l_sigmask_subcommand(WORD_LIST *list)
{
  sigset_t block_set, unblock_set, old;
  int opt;
  int any_opt = 0;
  int has_block = 0;
  int has_unblock = 0;

  sigemptyset(&block_set);
  sigemptyset(&unblock_set);

  l_enter_subcommand("sigmask", "[-s sigspec] [-u sigspec] [sigspec ...]", sigmask_doc);
  reset_internal_getopt();
  while ((opt = internal_getopt(list, "s:u:h")) != -1) {
    any_opt = 1;
    switch (opt) {
    case 's':
      if (strcasecmp(list_optarg, "all") == 0) {
        sigfillset(&block_set);
        has_block = 1;
      } else {
        int s = decode_signal(list_optarg, DSIG_NOCASE | DSIG_SIGPREFIX);
        if (s == NO_SIG) {
          sh_invalidsig(list_optarg);
          return (EXECUTION_FAILURE);
        }
        sigaddset(&block_set, s);
        has_block = 1;
      }
      break;
    case 'u':
      if (strcasecmp(list_optarg, "all") == 0) {
        sigfillset(&unblock_set);
        has_unblock = 1;
      } else {
        int s = decode_signal(list_optarg, DSIG_NOCASE | DSIG_SIGPREFIX);
        if (s == NO_SIG) {
          sh_invalidsig(list_optarg);
          return (EXECUTION_FAILURE);
        }
        sigaddset(&unblock_set, s);
        has_unblock = 1;
      }
      break;
    case 'h':
    case GETOPT_HELP:
      l_builtin_usage_long();
      return 0;
    default:
      builtin_usage();
      return (EX_USAGE);
    }
  }
  list = loptend;

  if (any_opt == 0 && list == 0) {
    sigemptyset(&block_set);
    if (sigprocmask(SIG_BLOCK, &block_set, &old) < 0) {
      builtin_error("sigprocmask: %s", strerror(errno));
      return (EXECUTION_FAILURE);
    }
    for (int i = 1; i < NSIG; i++) {
      if (sigismember(&old, i))
        printf("%s ", signal_name(i));
    }
    printf("\n");
    return (EXECUTION_SUCCESS);
  }

  if (list) {
    if (parse_sigspec(list, &block_set) < 0)
      return (EXECUTION_FAILURE);
    has_block = 1;
  }

  if (has_block) {
    if (sigprocmask(SIG_BLOCK, &block_set, &old) < 0) {
      builtin_error("sigprocmask (block): %s", strerror(errno));
      return (EXECUTION_FAILURE);
    }
  }

  if (has_unblock) {
    if (sigprocmask(SIG_UNBLOCK, &unblock_set, &old) < 0) {
      builtin_error("sigprocmask (unblock): %s", strerror(errno));
      return (EXECUTION_FAILURE);
    }
  }

  /* Update top_level_mask so it persists across command executions. */
  sigprocmask(SIG_BLOCK, NULL, &top_level_mask);

  return (EXECUTION_SUCCESS);
}

static COMMAND *L_make_bare_simple_command(void)
{
#ifndef L_BASH_VERSION
#error
#endif
#if L_BASH_VERSION > 50300
  return make_bare_simple_command(0); // line_number not used in our call
#else
  return make_bare_simple_command();
#endif
}

static const char *const sigunmask_doc[] = {
  "Unblock signals and run a command.",
  "",
  "L_builtin sigunmask [-h] -s sigspec cmd [args...]",
  "",
  "Temporarily unblocks the specified signal and executes the command.",
  "Note: -s takes a single sigspec; to unblock multiple signals use",
  "'-s ALL' or invoke sigunmask once per signal. ALL is case-insensitive.",
  "If the signal was pending, the trap is executed and the command is "
  "skipped.",
  "The command can be any shell command (builtin, function, or external).",
  "",
  "WARNING: There is a small window between unblocking and starting the "
  "command.",
  "If a signal arrives in this window, it may be delivered to the command "
  "itself",
  "rather than being caught by this builtin's check.",
  "",
  "Examples:",
  "  # Block INT for a critical step, but let the user Ctrl-C out of",
  "  # the sleep. -s takes ONE signal; 'TERM' here would become part of",
  "  # the command, not a second signal.",
  "  trap 'cancel=1' INT",
  "  L_builtin sigmask INT",
  "  while ! cancel; do",
  "    echo 'critical step'",
  "    L_builtin sigunmask -s INT sleep 1",
  "  done",
  "",
  "  # Unblock several signals at once with ALL.",
  "  trap 'cancel=1' INT TERM USR1",
  "  L_builtin sigmask INT TERM USR1",
  "  L_builtin sigunmask -s ALL sleep 5",
  "",
  "  # If the signal is already pending when sigunmask runs, the trap fires",
  "  # and the command is skipped (exit status is 128+signum).",
  "  trap 'echo caught' USR1",
  "  L_builtin sigmask -s USR1",
  "  L_raise -USR1",
  "  L_builtin sigunmask -s USR1 echo 'will not run'",
  "  # prints 'caught', exits 128+SIGUSR1",
  "",
  "Exit Status:",
  "Returns the status of the command, or 128+signum if a signal was caught.",
  (char *)NULL
};

int l_sigunmask_subcommand(WORD_LIST *list)
{
  sigset_t set, old, unblocked;
  int opt;

  sigemptyset(&unblocked);
  l_enter_subcommand("sigunmask", "[-h] -s sigspec cmd [args...]", sigunmask_doc);
  reset_internal_getopt();
  while ((opt = internal_getopt(list, "s:h")) != -1) {
    switch (opt) {
    case 's':
      if (strcasecmp(list_optarg, "all") == 0) {
        sigfillset(&unblocked);
      } else {
        int sig = decode_signal(list_optarg, DSIG_NOCASE | DSIG_SIGPREFIX);
        if (sig == NO_SIG) {
          sh_invalidsig(list_optarg);
          return (EXECUTION_FAILURE);
        }
        sigaddset(&unblocked, sig);
      }
      break;
    case 'h':
    case GETOPT_HELP:
      l_builtin_usage_long();
      return 0;
    default:
      builtin_usage();
      return (EX_USAGE);
    }
  }
  list = loptend;

  if (list == 0) {
    builtin_usage();
    return (EX_USAGE);
  }

  sigemptyset(&set);
  if (sigprocmask(SIG_BLOCK, &set, &old) < 0) {
    builtin_error("sigprocmask: %s", strerror(errno));
    return (EXECUTION_FAILURE);
  }

  sigset_t newmask = old;
  for (int i = 1; i < NSIG; i++) {
    if (sigismember(&unblocked, i))
      sigdelset(&newmask, i);
  }

  begin_unwind_frame("sigunmask");

  unwind_protect_mem((char *)&top_level_mask, sizeof(sigset_t));
  top_level_mask = newmask;

  sigset_t *pold = l_xmalloc(sizeof(sigset_t));
  *pold = old;
  add_unwind_protect(l_xfree, pold);
  add_unwind_protect(restore_process_sigmask, pold);

  if (sigprocmask(SIG_SETMASK, &newmask, NULL) < 0) {
    builtin_error("sigprocmask: %s", strerror(errno));
    run_unwind_frame("sigunmask");
    return (EXECUTION_FAILURE);
  }

  QUIT;

  int caught = 0;
  for (int i = 1; i < NSIG; i++) {
    if (sigismember(&unblocked, i) && pending_traps[i]) {
      caught = i;
      break;
    }
  }

  if (caught) {
    run_pending_traps();
    run_unwind_frame("sigunmask");
    return (128 + caught);
  }

  run_pending_traps();

  COMMAND *cmd = L_make_bare_simple_command();
  cmd->value.Simple->words = copy_word_list(list);

  int result = execute_command(cmd);

  dispose_command(cmd);

  run_unwind_frame("sigunmask");

  return (result);
}
