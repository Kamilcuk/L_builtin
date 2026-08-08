#ifndef L_BUILTIN_BASH_API_H
#define L_BUILTIN_BASH_API_H

#include <stddef.h>

/* Bash-compatible simple allocation wrappers (no file/line tracking) */
extern void *l_xmalloc(size_t size);
extern void *l_xrealloc(void *ptr, size_t size);
extern void l_xfree(void *ptr);
extern char *l_strdup(const char *p);

/* Override standard malloc/free/realloc to use our bash-compatible wrappers.
 * Undefine bash's macros first if they exist (from xmalloc.h via shell.h). */
#ifdef malloc
#undef malloc
#endif
#ifdef free
#undef free
#endif
#ifdef realloc
#undef realloc
#endif
#define malloc l_xmalloc
#define free l_xfree
#define realloc l_xrealloc

#endif /* L_BUILTIN_BASH_API_H */
