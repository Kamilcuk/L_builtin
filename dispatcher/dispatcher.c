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
#include <stdarg.h>

/* ------------------------------------------------------------------------- */

extern char *dist_version;
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

static int write_eintr(int fd, const char *data, size_t len)
{
  size_t pos = 0;
  while (pos < len) {
    ssize_t n = write(fd, data + pos, len - pos);
    if (n < 0) {
      if (errno == EINTR)
        continue;
      return -1;
    }
    pos += (size_t)n;
  }
  return 0;
}

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
    if (write_eintr(fd, out_buf, out.pos)) {
      fprintf(stderr, "L_builtin: write: %s\n", strerror(errno));
      ZSTD_freeDStream(ds);
      return ZSTD_error_dstBuffer_null;
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

static int sh(const char *fmt, ...)
{
  va_list va;
  va_start(va, fmt);
  char buf[128];
  int ret = vsnprintf(buf, sizeof(buf), fmt, va);
  if (ret < 0 || ret >= (int)sizeof(buf)) {
    fprintf(stderr, "%s: Could not run command: %s!\n", __FILE__, fmt);
    return -1;
  }
  va_end(va);
  fprintf(stderr, "+ %s\n", buf);
  return system(buf);
}

/* ------------------------------------------------------------------------- */

struct embedded_so {
  const char *version;
  const unsigned char *start;
  size_t size;
};

#define LEN(x) sizeof(x) / sizeof(*x)

#include "dispatcher_version_table_data.h"

static void *load_and_decompress_embedded_so(const char *version)
{
  for (int i = 0; i < LEN(embedded_sos); i++) {
    const struct embedded_so *const e = &embedded_sos[i];
    if (strcmp(e->version, version) == 0) {
      int fd = memfd_create("L_builtin_embedded", 0); // MFD_CLOEXEC);
      if (fd < 0) {
        perror("L_builtin: memfd_create");
        return NULL;
      }
      if (zstd_decompress_to_fd(e->start, e->size, fd) != 0) {
        return NULL;
      }
      char path[64];
      snprintf(path, sizeof(path), "/proc/self/fd/%d", fd);
      sh("nm -D -g --defined-only /proc/self/fd/3 | grep L_builtin_struct");
      sh("readelf -Ws /proc/self/fd/3 | grep L_builtin_struct");
      sh("readelf -Ws --dyn-syms /proc/self/fd/3 | grep L_builtin_struct");
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

struct version {
  int major;
  int minor;
};

static struct version version_from_string(const char *s)
{
  const struct version err = {-1, -1};
  struct version v = {0, 0};
  if (!s)
    return err;
  while (*s >= '0' && *s <= '9')
    v.major = v.major * 10 + (*s++ - '0');
  if (*s++ != '.')
    return err;
  while (*s >= '0' && *s <= '9')
    v.minor = v.minor * 10 + (*s++ - '0');
  return v;
}

struct builtin L_builtin_struct;

static int l_entrypoint(struct word_list *list)
{
  struct version v = version_from_string(dist_version);
  char version[16];
  snprintf(version, sizeof(version), "%d.%d", v.major, v.minor);
  void *handle = load_and_decompress_embedded_so(version);
  if (!handle) {
    fprintf(stderr, "L_builtin: no module for bash %s\n", version);
    return 1;
  }
  char symbol[64];
  snprintf(symbol, sizeof(symbol), "L_builtin_struct_embedded", v.major, v.minor);
  const struct builtin *b = dlsym(handle, symbol);
  if (!b) {
    fprintf(stderr, "L_builtin: no %s symbol in module version %s\n", symbol, version);
    return 1;
  }
  fprintf(stderr, "LONGDOC: %s %s\n", L_builtin_struct.long_doc[0], b->long_doc[0]);
  // Overwrite the structure with the actual imported data.
  L_builtin_struct.short_doc = b->short_doc;
  L_builtin_struct.long_doc = b->long_doc;
  L_builtin_struct.function = b->function;
  return b->function(list);
}

static const char *const l_builtin_doc[] = {
  "L_builtin multi-version dispatcher.",
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
