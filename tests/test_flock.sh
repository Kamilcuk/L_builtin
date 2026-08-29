# Tests for the `L_builtin flock` subcommand: flock(2) on an existing fd.

_L_test_flock_exclusive() {
    L_builtin memfd MF
    L_unittest_checkexit 0 L_builtin flock -x "$MF"
    L_unittest_checkexit 0 L_builtin flock -u "$MF"
    L_builtin close "$MF"
}

_L_test_flock_shared() {
    L_builtin memfd MF
    L_unittest_checkexit 0 L_builtin flock -s "$MF"
    L_unittest_checkexit 0 L_builtin flock -u "$MF"
    L_builtin close "$MF"
}

_L_test_flock_unlock() {
    L_builtin memfd MF
    L_unittest_checkexit 0 L_builtin flock -u "$MF"
    L_builtin close "$MF"
}

_L_test_flock_nonblock() {
    L_builtin memfd MF
    L_unittest_checkexit 0 L_builtin flock -n -x "$MF"
    L_unittest_checkexit 0 L_builtin flock -u "$MF"
    L_builtin close "$MF"
}

_L_test_flock_mutually_exclusive() {
    # -x and -s together are an error.
    L_builtin memfd MF
    L_unittest_checkexit 2 L_builtin flock -x -s "$MF"
    L_builtin close "$MF"
}

_L_test_flock_invalid_fd() {
    L_unittest_checkexit 1 L_builtin flock -x 999999
}

_L_test_flock_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin flock -h
}

_L_test_flock_help_long() {
    L_unittest_cmd -j -r "flock\(2\)" L_builtin flock --help
}
