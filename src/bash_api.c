#include <config.h>
#include <errno.h>
#include <stddef.h>
#include <sys/types.h>
#include <shell.h>
#include <variables.h>
#include <array.h>
#include <command.h>
#include <general.h>
#include <subst.h>

/* Only bridge what Rust CANNOT call directly without knowing struct layouts */

ARRAY_ELEMENT *l_array_head(ARRAY *a) { return a->head; }

ARRAY_ELEMENT *l_element_forw(ARRAY_ELEMENT *ae) { return element_forw(ae); }

char *l_element_value(ARRAY_ELEMENT *ae) { return element_value(ae); }

WORD_LIST *l_word_list_next(WORD_LIST *list) { return list->next; }

WORD_DESC *l_word_list_word(WORD_LIST *list) { return list->word; }

char *l_word_desc_string(WORD_DESC *word) { return word->word; }

/* Wrapper for readonly_p macro - checks if variable has readonly attribute */
int l_readonly_p(SHELL_VAR *var) { return readonly_p(var); }

/* Wrapper for invisible_p macro - checks if variable has invisible attribute */
int l_invisible_p(SHELL_VAR *var) { return invisible_p(var); }

/* Wrapper for array_p macro - checks if variable is an array */
int l_array_p(SHELL_VAR *var) { return array_p(var); }

/* Wrapper for array_cell macro - gets array pointer from variable */
ARRAY *l_array_cell(SHELL_VAR *var) { return array_cell(var); }

/* Wrapper for value_cell macro - gets value pointer from variable */
char *l_value_cell(SHELL_VAR *var) { return value_cell(var); }

/* Expand a string with double-quote semantics and return an owned string
 * (caller must free with free()) */
char *l_expand_string_to_string_in_quotes(const char *string)
{
  return expand_string_to_string((char *)string, Q_DOUBLE_QUOTES);
}
