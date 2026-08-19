/* cmd_fcntl.c - flag lookup tables for the fcntl subcommand.
 *
 * The fcntl(2) implementation lives in Rust (src/cmd_fcntl.rs).  This
 * translation unit only provides the name->value flag tables that are shared
 * between the C side (bindgen-generated bindings) and the Rust side.
 *
 * Each array is sentinel-terminated ({NULL, 0}) so that Rust can iterate over
 * it at runtime without knowing the compile-time element count (which varies
 * with the #ifdef guards below).
 */

#include <config.h>
#include <fcntl.h>

#include "bash_api.h"

/*
 * open(2) flag names, used to parse the comma-separated FLAGS argument of
 * `fcntl setfl`.  Entries are conditionally compiled - a flag not defined on
 * this platform simply does not appear, so the Rust parser rejects it with a
 * clear "unknown flag" error.  The F_GETFL/F_SETFL operations deal with file
 * *status* flags, not access modes, but O_RDONLY/O_WRONLY/O_RDWR are listed
 * too so that full F_GETFL results can be decoded.
 */
const l_flag_entry_t l_open_flags[] = {
#ifdef O_RDONLY
  {"rdonly", O_RDONLY},
#endif
#ifdef O_WRONLY
  {"wronly", O_WRONLY},
#endif
#ifdef O_RDWR
  {"rdwr", O_RDWR},
#endif
#ifdef O_CREAT
  {"creat", O_CREAT},
#endif
#ifdef O_EXCL
  {"excl", O_EXCL},
#endif
#ifdef O_NOCTTY
  {"noctty", O_NOCTTY},
#endif
#ifdef O_TRUNC
  {"trunc", O_TRUNC},
#endif
#ifdef O_APPEND
  {"append", O_APPEND},
#endif
#ifdef O_NONBLOCK
  {"nonblock", O_NONBLOCK},
#endif
#ifdef O_NDELAY
  {"ndelay", O_NDELAY},
#endif
#ifdef O_SYNC
  {"sync", O_SYNC},
#endif
#ifdef O_DSYNC
  {"dsync", O_DSYNC},
#endif
#ifdef O_RSYNC
  {"rsync", O_RSYNC},
#endif
#ifdef O_ASYNC
  {"async", O_ASYNC},
#endif
#ifdef O_DIRECT
  {"direct", O_DIRECT},
#endif
#ifdef O_DIRECTORY
  {"directory", O_DIRECTORY},
#endif
#ifdef O_NOFOLLOW
  {"nofollow", O_NOFOLLOW},
#endif
#ifdef O_NOATIME
  {"noatime", O_NOATIME},
#endif
#ifdef O_CLOEXEC
  {"cloexec", O_CLOEXEC},
#endif
#ifdef O_PATH
  {"path", O_PATH},
#endif
#ifdef O_TMPFILE
  {"tmpfile", O_TMPFILE},
#endif
#ifdef O_LARGEFILE
  {"largefile", O_LARGEFILE},
#endif
  {NULL, 0} /* sentinel */
};

/*
 * File-descriptor flag names, used to parse the comma-separated FLAGS argument
 * of `fcntl setfd`.  F_GETFD/F_SETFD deal with per-fd flags such as
 * FD_CLOEXEC.
 */
const l_flag_entry_t l_fd_flags[] = {
#ifdef FD_CLOEXEC
  {"cloexec", FD_CLOEXEC},
#endif
  {NULL, 0} /* sentinel */
};
