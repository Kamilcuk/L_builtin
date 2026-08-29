# Tests for the `L_builtin memfd` subcommand: memfd_create(2) anonymous fds.

_L_test_memfd_create() {
    L_builtin memfd MF
    L_unittest_cmd -jr '^[0-9]+$' echo "$MF"
    # The fd is a real file in RAM: write and read back through it.
    printf 'hello memfd' >&"$MF"
    L_builtin lseek -v pos "$MF" 0 SET
    local got
    L_builtin read -v got "$MF" 11
    L_unittest_eq "$got" "hello memfd"
    L_builtin close "$MF"
}

_L_test_memfd_named() {
    L_builtin memfd MF mydata
    L_unittest_cmd -jr '^[0-9]+$' echo "$MF"
    L_builtin close "$MF"
}

_L_test_memfd_no_cloexec() {
    # -C clears close-on-exec; the command should still succeed.
    L_builtin memfd -C MF
    L_unittest_cmd -jr '^[0-9]+$' echo "$MF"
    L_builtin close "$MF"
}

_L_test_memfd_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin memfd -h
}

_L_test_memfd_help_long() {
    L_unittest_cmd -j -r "memfd_create" L_builtin memfd --help
}
