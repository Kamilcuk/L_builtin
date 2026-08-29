# Tests for the `L_builtin sedvar` subcommand: run a sed script (sed-rs) over a
# bash variable in place, with each array/assoc element as one NUL-delimited
# record (sed -z semantics) so element boundaries survive embedded newlines.

_L_test_sedvar_array_subst() {
    local arr=( foo bar baz )
    L_unittest_cmd -c L_builtin sedvar arr 's/a/@/'
    L_unittest_eq "${arr[*]}" "foo b@r b@z"
}

_L_test_sedvar_array_delete() {
    local arr=( a b c )
    L_unittest_cmd -c L_builtin sedvar arr '/b/d'
    L_unittest_eq "${arr[*]}" "a c"
}

_L_test_sedvar_scalar() {
    local v="hello"
    L_unittest_cmd -c L_builtin sedvar v 's/hello/world/'
    L_unittest_eq "$v" "world"
}

_L_test_sedvar_assoc() {
    declare -A h=( [x]=foo [y]=bar )
    L_unittest_cmd -c L_builtin sedvar h 's/a/A/'
    L_unittest_eq "${h[x]}" "foo"
    L_unittest_eq "${h[y]}" "bAr"
}

_L_test_sedvar_address_print() {
    # Non-quiet: auto-print all lines, plus an explicit `2p` duplicates line 2.
    local arr=( x y z )
    L_unittest_cmd -c L_builtin sedvar arr '2p'
    L_unittest_eq "${arr[*]}" "x y y z"
}

_L_test_sedvar_newline_safe() {
    # An element containing a newline stays a single element (NUL records).
    local arr=( "a b" "c d" )
    L_unittest_cmd -c L_builtin sedvar arr 's/ /_/'
    L_unittest_eq "${arr[*]}" "a_b c_d"
}

_L_test_sedvar_invalid_script() {
    local v="abc"
    L_unittest_cmd -e 2 L_builtin sedvar v 's/[invalid/x/'
    L_unittest_eq "$v" "abc"
}

_L_test_sedvar_missing_var() {
    L_unittest_cmd -e 1 L_builtin sedvar DOES_NOT_EXIST 's/x/y/'
}
