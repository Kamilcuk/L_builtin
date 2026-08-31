#include <config.h>
#include <errno.h>
#include <poll.h>
#include <signal.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/time.h>
#include <unistd.h>

#include "bash_api.h"
#include "L_builtin.h"

static short parse_events(const char *s)
{
  short ev = 0;
  if (!s || !*s)
    return POLLIN;
  while (*s) {
    switch (*s) {
    case 'r':
      ev |= POLLIN;
      break;
    case 'w':
      ev |= POLLOUT;
      break;
    case 'p':
      ev |= POLLPRI;
      break;
    }
    s++;
  }
  return ev ? ev : POLLIN;
}

static char *format_revents(short revents)
{
  static char buf[8];
  int p = 0;
  if (revents & POLLIN)
    buf[p++] = 'r';
  if (revents & POLLOUT)
    buf[p++] = 'w';
  if (revents & POLLPRI)
    buf[p++] = 'p';
  if (revents & POLLHUP)
    buf[p++] = 'h';
  if (revents & POLLERR)
    buf[p++] = 'e';
  if (revents & POLLNVAL)
    buf[p++] = 'n';
  buf[p] = '\0';
  return buf;
}

static int
do_poll(struct pollfd *pfds, int nfds, struct timespec *tsp, sigset_t *sigmask, int is_ppoll)
{
#if HAVE_PPOLL
  if (is_ppoll)
    return ppoll(pfds, nfds, tsp, sigmask);
  else
#endif
  {
    int timeout = -1;
    if (tsp)
      timeout = (int)(tsp->tv_sec * 1000 + tsp->tv_nsec / 1000000);
    return poll(pfds, nfds, timeout);
  }
}

static int poll_internal(WORD_LIST *list, int is_ppoll)
{
  char *ret_var = NULL;
  char *timeout_str = NULL;
  sigset_t unblock_set, current_mask, new_mask;
  int opt, unblock_any = 0;
  int interruptible = 0;

  sigemptyset(&unblock_set);
  reset_internal_getopt();

  const char *optstr = is_ppoll ? "t:v:u:ih" : "t:v:ih";

  while ((opt = internal_getopt(list, (char *)optstr)) != -1) {
    switch (opt) {
    case 't':
      timeout_str = list_optarg;
      break;
    case 'v':
      ret_var = list_optarg;
      break;
    case 'u':
      if (strcasecmp(list_optarg, "all") == 0) {
        sigfillset(&unblock_set);
        unblock_any = 1;
      } else {
        int sig = decode_signal(list_optarg, DSIG_NOCASE | DSIG_SIGPREFIX);
        if (sig == NO_SIG) {
          sh_invalidsig(list_optarg);
          return (EXECUTION_FAILURE);
        }
        sigaddset(&unblock_set, sig);
        unblock_any = 1;
      }
      break;
    case 'i':
      interruptible = 1;
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

  int nfds = 0;
  WORD_LIST *l;
  for (l = list; l; l = l->next)
    nfds++;

  struct pollfd *pfds = NULL;
  if (nfds > 0) {
    pfds = (struct pollfd *)l_xmalloc(nfds * sizeof(struct pollfd));
    l = list;
    for (int i = 0; i < nfds; i++, l = l->next) {
      char *s = l->word->word;
      char *sep = strchr(s, ':');
      if (sep) {
        *sep = '\0';
        pfds[i].fd = atoi(s);
        pfds[i].events = parse_events(sep + 1);
        *sep = ':';
      } else {
        pfds[i].fd = atoi(s);
        pfds[i].events = POLLIN;
      }
      pfds[i].revents = 0;
    }
  }

  struct timespec ts, *tsp = NULL;
  if (timeout_str) {
    double t = atof(timeout_str);
    ts.tv_sec = (long)t;
    ts.tv_nsec = (long)((t - (long)t) * 1000000000);
    tsp = &ts;
  }

  sigemptyset(&new_mask);
  if (is_ppoll) {
    if (sigprocmask(SIG_BLOCK, NULL, &current_mask) < 0) {
      builtin_error("sigprocmask: %s", strerror(errno));
      if (pfds)
        l_xfree(pfds);
      return (EXECUTION_FAILURE);
    }
    new_mask = current_mask;
    if (unblock_any) {
      for (int i = 1; i < NSIG; i++)
        if (sigismember(&unblock_set, i))
          sigdelset(&new_mask, i);
    }
  }

  int ret = do_poll(pfds, nfds, tsp, &new_mask, is_ppoll);

  /* Retry on EINTR - poll was interrupted by a signal, unless -i flag is set */
  while (ret < 0 && errno == EINTR && !interruptible) {
    ret = do_poll(pfds, nfds, tsp, &new_mask, is_ppoll);
  }

  if (ret < 0)
    builtin_error("poll: %s", strerror(errno));

  if (ret_var) {
    /* Prepare the target variable as an indexed array (auto-converting a
     * scalar/associative one in place, and clearing the att_invisible flag that
     * make_local_variable sets on unset locals so the values we write persist). */
    ARRAY *a = l_prepare_indexed_array(ret_var);
    if (a == NULL) {
      builtin_error("%s: cannot create array", ret_var);
      if (pfds)
        l_xfree(pfds);
      return (EXECUTION_FAILURE);
    }

    if (ret > 0) {
      for (int i = 0; i < nfds; i++) {
        if (pfds[i].revents) {
          /* Sparse indexed array: the fd is the index, the value is just the
           * decoded readiness tokens (e.g. 'r', 'rw') - mirrors epoll_wait. */
          array_insert(a, pfds[i].fd, format_revents(pfds[i].revents));
        }
      }
    }
  }

  if (pfds)
    l_xfree(pfds);
  return (ret >= 0 ? EXECUTION_SUCCESS : EXECUTION_FAILURE);
}

static const char *const poll_doc[] = {
  "Wait for file descriptors to become ready.",
  "",
  "L_builtin poll [-t TIMEOUT] [-v ARRAY_VAR] [-i] [FD[:EVENTS] ...]",
  "",
  "Poll file descriptors using poll(2). EVENTS can be 'r', 'w', or 'p'.",
  "Results are stored in the indexed array ARRAY_VAR as ARR[fd]=events: the fd",
  "is the array index and the value is the decoded readiness tokens (e.g.",
  "ARR[3]=\"r\", ARR[5]=\"rw\"). This sparse format matches the `epoll wait`",
  "subcommand, so a readiness loop works against either.",
  "REVENTS contains 'r', 'w', 'p', 'h' (hangup), 'e' (error), or 'n' ",
  "(invalid).",
  "",
  "If -i is provided, poll will not automatically retry on signal interruption",
  "(EINTR). Instead, it will fail with an error. By default, poll retries on",
  "EINTR.",
  "",
  "Example:",
  "  L_builtin pipe in",
  "  L_builtin pipe out",
  "  L_builtin timerfd t 500ms",
  "  printf 'hello' >&\"${in[1]}\" &",
  "  L_builtin poll -t 2 -v res \"${in[0]}:r\" \"${out[0]}:w\" \"$t:r\"",
  "  for fd in \"${!res[@]}\"; do",
  "    rev=${res[fd]}",
  "    echo \"fd $fd ready: $rev\"",
  "    [[ $rev == *r* ]] && echo \"  fd $fd is readable\"",
  "    [[ $rev == *w* ]] && echo \"  fd $fd is writable\"",
  "    [[ $rev == *h* ]] && echo \"  fd $fd hung up\"",
  "    [[ $rev == *e* ]] && echo \"  fd $fd errored\"",
  "  done",
  "  exec {in[0]}<&- {in[1]}>&- {out[0]}<&- {out[1]}>&- {t}<&-",
  "",
  "Exit Status:",
  "Returns success if poll succeeds, even if it timed out. Returns failure "
  "on",
  "system errors.",
  NULL
};

int poll_subcommand(WORD_LIST *list)
{
  l_enter_subcommand("poll", "[-t TIMEOUT] [-v ARRAY_VAR] [-i] [FD[:EVENTS] ...]", poll_doc);
  return poll_internal(list, 0);
}

#if HAVE_PPOLL

static const char *const ppoll_doc[] = {
  "Wait for file descriptors and unblock signals atomically.",
  "",
  "L_builtin ppoll [-t TIMEOUT] [-v ARRAY_VAR] [-u SIGSPEC] [-i] [FD[:EVENTS] ...]",
  "",
  "Poll file descriptors and unblock signals using ppoll(2).",
  "Results are stored in the indexed array ARRAY_VAR as ARR[fd]=events: the fd",
  "is the array index and the value is the decoded readiness tokens (e.g.",
  "ARR[3]=\"r\", ARR[5]=\"rw\"). This sparse format matches the `epoll wait`",
  "subcommand, so a readiness loop works against either.",
  "",
  "Use -u SIGSPEC to temporarily unblock specified signals during ppoll.",
  "Use -u 'ALL' (case-insensitive) to unblock all signals.",
  "",
  "If -i is provided, ppoll will not automatically retry on signal interruption",
  "(EINTR). Instead, it will fail with an error. By default, ppoll retries on",
  "EINTR.",
  "",
  "EVENTS and REVENTS format:",
  "  EVENTS can be a combination of 'r' (read, default if omitted),",
  "  'w' (write), or 'p' (priority).",
  "  REVENTS contains 'r', 'w', 'p', 'h' (hangup), 'e' (error), or 'n' "
  "(invalid).",
  "",
  "Example:",
  "  # Poll fd 0 for reading with a 2.5 second timeout, unblocking all "
  "signals",
  "  L_builtin ppoll -t 2.5 -v results -u ALL 0:r",
  "",
  "Exit Status:",
  "Returns success if ppoll succeeds. Returns failure on system errors.",
  NULL
};

int ppoll_subcommand(WORD_LIST *list)
{
  l_enter_subcommand(
    "poll", "[-t TIMEOUT] [-v ARRAY_VAR] [-u SIGSPEC] [-i] [FD[:EVENTS] ...]", ppoll_doc
  );
  return poll_internal(list, 1);
}
#endif
