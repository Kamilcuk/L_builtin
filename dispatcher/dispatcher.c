#define _GNU_SOURCE
#include <dlfcn.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <unistd.h>
#include <zstd.h>
#include <errno.h>

/* ------------------------------------------------------------------------- */
extern char *dist_version;
extern int patch_level;
extern int build_version;
extern char *release_status;
struct word_list;
struct builtin {
  const char *name;
  int (*function)(struct word_list *);
  int flags;
  const char **long_doc;
  const char *short_doc;
  char *handle;
};
#define BUILTIN_ENABLED 1
/* ------------------------------------------------------------------------- */

struct embedded_so {
  const char *version;
  const unsigned char *start;
  const unsigned char *end;
};

#include "dispatcher_version_table_data.h"

static size_t zstd_decompress_to_fd(const char *start, size_t size, int fd)
{
  ZSTD_DStream *ds = ZSTD_createDStream();
  if (!ds) {
    fprintf(stderr, "L_builtin: ZSTD_createDStream: out of memory\n");
    return ZSTD_error_memory_allocation;
  }
  size_t ret = ZSTD_initDStream(ds);
  if (ZSTD_isError(ret)) {
    fprintf(stderr, "L_builtin: ZSTD_initDStream: %s\n", ZSTD_getErrorName(ret));
    ZSTD_freeDStream(ds);
    return ret;
  }
  char out_buf[ZSTD_DStreamOutSize()];
  ZSTD_inBuffer in = {
    .src = start,
    .size = size,
    .pos = 0,
  };
  for (;;) {
    ZSTD_outBuffer out = {
      .dst = out_buf,
      .size = sizeof(out_buf),
      .pos = 0,
    };
    ret = ZSTD_decompressStream(ds, &out, &in);
    if (ZSTD_isError(ret)) {
      fprintf(stderr, "L_builtin: ZSTD_decompressStream: %s\n", ZSTD_getErrorName(ret));
      ZSTD_freeDStream(ds);
      return ret;
    }
    for (size_t pos = 0; pos < out.pos;) {
      ssize_t n = write(fd, out_buf + pos, out.pos - pos);
      if (n < 0) {
        if (errno == EINTR)
          continue;
        fprintf(stderr, "L_builtin: write: %s\n", strerror(errno));
        ZSTD_freeDStream(ds);
        return ZSTD_error_dstBuffer_null;
      }
      pos += (size_t)n;
    }
    if (ret == 0) {
      if (in.pos == in.size)
        break;
      ret = ZSTD_initDStream(ds);
      if (ZSTD_isError(ret)) {
        fprintf(stderr, "L_builtin: ZSTD_initDStream: %s\n", ZSTD_getErrorName(ret));
        ZSTD_freeDStream(ds);
        return ret;
      }
    }
    if (in.pos == in.size && out.pos == 0) {
      fprintf(stderr, "L_builtin: ZSTD_decompressStream: truncated input\n");
      ZSTD_freeDStream(ds);
      return ZSTD_error_corruption_detected;
    }
  }
  ZSTD_freeDStream(ds);
  return 0;
}

#define LEN(x) sizeof(x) / sizeof(*x)

static void *load_and_decompress_embedded_so(const char *version)
{
  for (int i = 0; i < LEN(embedded_sos); i++) {
    if (strcmp(embedded_sos[i].version, version) == 0) {
      const unsigned char *compressed_start = embedded_sos[i].start;
      const unsigned char *compressed_end = embedded_sos[i].end;
      size_t compressed_size = compressed_end - compressed_start;
      int fd = memfd_create("L_builtin_embedded", MFD_CLOEXEC);
      if (fd < 0) {
        perror("L_builtin: memfd_create");
        return NULL;
      }
      if (zstd_decompress_to_fd(compressed_start, compressed_size, fd) != 0) {
        return NULL;
      }
      char path[64];
      snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
      void *h = dlopen(path, RTLD_NOW | RTLD_LOCAL);
      if (!h) {
        fprintf(stderr, "L_builtin: dlopen(%s) failed: %s\n", version, dlerror());
        return NULL;
      }
      return h;
    }
  }
  return NULL;
}

static const char *get_runtime_bash_version(void)
{
  static char version[16];
  int major = 0, minor = 0;
  if (dist_version) {
    major = atoi(dist_version);
  }
  minor = patch_level / 100;
  snprintf(version, sizeof(version), "%d.%d", major, minor);
  return version;
}

static int l_entrypoint(struct word_list *list)
{
  static void *handle = NULL;
  if (!handle) {
    const char *version = get_runtime_bash_version();
    handle = load_and_decompress_embedded_so(version);
    if (!handle) {
      fprintf(stderr, "L_builtin: no module for bash %s\n", version);
      return 1;
    }
  }
  int (*ep)(struct word_list *) = dlsym(handle, "l_entrypoint");
  if (!ep) {
    fprintf(stderr, "L_builtin: no l_entrypoint in module\n");
    return 1;
  }
  return ep(list);
}

static const char *const l_builtin_doc[] = {
  "L_builtin multi-version dispatcher (zstd compressed).",
  "",
  "L_builtin <subcommand> [options] [args]",
  "",
  "Available subcommands:",
  "  version      Print build and bash version information",
  NULL
};

struct builtin L_builtin_struct = {
  "L_builtin",
  l_entrypoint,
  BUILTIN_ENABLED,
  (void *)l_builtin_doc,
  "L_builtin [-v <var>] <subcommand> [options] [args...]",
  0
};
