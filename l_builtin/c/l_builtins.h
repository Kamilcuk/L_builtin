#ifndef L_BUILTINS_H_
#define L_BUILTINS_H_

#include "l_bash_api.h"
#include <fcntl.h>

/*
 * Flag lookup tables for the fcntl subcommand (defined in cmd_fcntl.c).
 * Each entry maps a human-readable flag name to its numeric constant.
 * The arrays are sentinel-terminated ({NULL, 0}) so Rust can iterate
 * without a compile-time length.
 */
typedef struct {
  const char *name;
  int flag;
} l_flag_entry_t;

extern const l_flag_entry_t l_open_flags[];
extern const l_flag_entry_t l_fd_flags[];

int l_poll_subcommand(WORD_LIST *list);
#if HAVE_PPOLL
int l_ppoll_subcommand(WORD_LIST *list);
#endif
int l_sigmask_subcommand(WORD_LIST *list);
int l_sigunmask_subcommand(WORD_LIST *list);
int l_cmd_ext(WORD_LIST *list);

#endif
