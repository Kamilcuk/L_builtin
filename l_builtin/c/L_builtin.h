#ifndef L_BUILTIN_H_
#define L_BUILTIN_H_

#include "l_bash_api.h"

extern const char *const L_builtin_doc[];
extern struct builtin L_builtin_struct;
extern const struct builtin *const L_builtin_impl;

int l_poll_subcommand(WORD_LIST *list);
#if HAVE_PPOLL
int l_ppoll_subcommand(WORD_LIST *list);
#endif
int l_sigmask_subcommand(WORD_LIST *list);
int l_sigunmask_subcommand(WORD_LIST *list);
int l_cmd_ext(WORD_LIST *list);

// comes from rust
int l_entrypoint(WORD_LIST *list);

#endif
