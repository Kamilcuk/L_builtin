#ifndef L_BUILTIN_BASH_API_H
#define L_BUILTIN_BASH_API_H

#include <stddef.h>

/* Bash-compatible simple allocation wrappers (no file/line tracking) */
extern void *l_xmalloc(size_t size);
extern void *l_xrealloc(void *ptr, size_t size);
extern void l_xfree(void *ptr);

#endif /* L_BUILTIN_BASH_API_H */
