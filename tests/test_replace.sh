# Tests for the `L_builtin replace` subcommand: a regex substitution applied
# in place to a bash variable's value(s), built on the `regex` crate (byte mode).

_L_test_replace_scalar() {
    local v="foobar"
    L_unittest_cmd -c L_builtin replace v o 0
    L_unittest_eq "$v" "f00bar"
}

_L_test_replace_array() {
    local arr=( foo bar baz )
    L_unittest_cmd -c L_builtin replace arr "a" "@"
    L_unittest_eq "${arr[*]}" "foo b@r b@z"
}

_L_test_replace_assoc() {
    declare -A h=( [k1]=foo [k2]=bar )
    L_unittest_cmd -c L_builtin replace h "a" "A"
    L_unittest_eq "${h[k1]}" "foo"
    L_unittest_eq "${h[k2]}" "bAr"
}

_L_test_replace_capture_group() {
    local v="2024-01-02"
    L_unittest_cmd -c L_builtin replace v '([0-9]+)-([0-9]+)' '$2/$1'
    L_unittest_eq "$v" "01/2024-02"
}

_L_test_replace_global() {
    local v="a.b.c"
    L_unittest_cmd -c L_builtin replace v '\.' '/'
    L_unittest_eq "$v" "a/b/c"
}

_L_test_replace_invalid_pattern() {
    local v="abc"
    L_unittest_cmd -e 2 L_builtin replace v "(" x
    L_unittest_eq "$v" "abc"
}

_L_test_replace_missing_var() {
    L_unittest_cmd -e 1 L_builtin replace DOES_NOT_EXIST x y
}
