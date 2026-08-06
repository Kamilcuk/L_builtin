#include <stdbool.h>
#include <stdio.h>
#include <boost/preprocessor/seq/for_each.h>
#include <boost/preprocessor/tuple/to_seq.h>
#include <boost/preprocessor/stringize.h>

// Declare struct fields: bool a; / const char * d;
#define GETOPT_FIELD(r, type, elem) \
    type elem;

// Stringify flag/option characters for getopt spec string: "a" / "d:"
#define GETOPT_SPEC(r, suffix, elem) \
    BOOST_PP_STRINGIZE(elem) suffix

// Generate switch case branches: case 'a': opts.a = val; break;
#define GETOPT_CASE(r, val, elem) \
    case BOOST_PP_STRINGIZE(elem)[0]: opts.elem = val; break;

#define GETOPT_EACH(macro, data, seq) \
    BOOST_PP_SEQ_FOR_EACH(macro, data, seq)

#define GETOPT(list, flags, options) \
    struct { \
        GETOPT_EACH(GETOPT_FIELD, bool, BOOST_PP_VARIADIC_TO_SEQ flags) \
        GETOPT_EACH(GETOPT_FIELD, const char *, BOOST_PP_VARIADIC_TO_SEQ options) \
    } opts = {0}; \
    do { \
        reset_internal_getopt(); \
        const char *const spec = \
            GETOPT_EACH(GETOPT_SPEC, "", BOOST_PP_VARIADIC_TO_SEQ flags) \
            GETOPT_EACH(GETOPT_SPEC, ":", BOOST_PP_VARIADIC_TO_SEQ options) \
            "h"; \
        for (int opt; (opt = internal_getopt(list, spec)) != -1; ) { \
            switch (opt) { \
                case 'h': builtin_usage(); return (EX_USAGE); \
                GETOPT_EACH(GETOPT_CASE, true, BOOST_PP_VARIADIC_TO_SEQ flags) \
                GETOPT_EACH(GETOPT_CASE, list_optarg, BOOST_PP_VARIADIC_TO_SEQ options) \
                default: builtin_usage(); return (EX_USAGE); \
            } \
        } \
    } while(0)

/*
int example(WORD_LIST *list) {
    GETOPT(list, (a, b, c), (d, e, f));
    printf("Flag -a=%d -d=%s\n", opts.a, opts.d);
}
*/
