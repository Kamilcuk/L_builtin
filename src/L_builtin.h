#ifndef L_BUILTIN_H
#define L_BUILTIN_H

struct word_list;

/* Top-level entry point, implemented in Rust (src/entrypoint.rs). */
int l_entrypoint(struct word_list *list);

int lseek_subcommand(struct word_list *list);
int poll_subcommand(struct word_list *list);
#if HAVE_PPOLL
int ppoll_subcommand(struct word_list *list);
#endif
int sigmask_subcommand(struct word_list *list);
int sigunmask_subcommand(struct word_list *list);
int pipe_subcommand(struct word_list *list);
int listen_subcommand(struct word_list *list);
int accept_subcommand(struct word_list *list);
int connect_subcommand(struct word_list *list);
int shutdown_subcommand(struct word_list *list);
int send_subcommand(struct word_list *list);
int recv_subcommand(struct word_list *list);
int sleep_subcommand(struct word_list *list);

#endif
