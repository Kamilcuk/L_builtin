# Tests for the `L_builtin ext` subcommand: dispatch to bash loadable builtins.

_L_test_ext_help() {
    L_unittest_cmd -j -r "usage" L_builtin ext -h
}

_L_test_ext_hello() {
    L_unittest_cmd -j -r "hello world" L_builtin ext hello
}

_L_test_ext_false() {
    # The `false` loadable builtin returns failure.
    L_unittest_checkexit 1 L_builtin ext false
}

_L_test_ext_unknown() {
    L_unittest_checkexit 2 L_builtin ext bogus
}
