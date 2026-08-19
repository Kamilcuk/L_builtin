# Tests for the `L_builtin eventfd` subcommand group: eventfd(2) counter fds.

_L_test_eventfd_create_write_read() {
    L_builtin eventfd create -n ev
    [[ "$ev" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$ev'"

    # write 5, then read → counter is 5, reset to 0.
    L_builtin eventfd write "$ev" 5
    L_builtin eventfd read "$ev" val
    L_unittest_eq "$val" 5

    # write 3, read → 3.
    L_builtin eventfd write "$ev" 3
    L_builtin eventfd read "$ev" val
    L_unittest_eq "$val" 3

    # default write value (1) and default read (prints).
    L_builtin eventfd write "$ev"
    L_unittest_cmd -jr "1" L_builtin eventfd read "$ev"

    L_builtin close "$ev"
}

_L_test_eventfd_apply_set_and_drain() {
    L_builtin eventfd create -n ev

    # write 5 (counter = 5)
    L_builtin eventfd write "$ev" 5
    # apply 3 → counter set to exactly 3 (not 5+3=8)
    L_builtin eventfd apply "$ev" 3
    L_builtin eventfd read "$ev" val
    L_unittest_eq "$val" 3

    # apply 0 → drain counter to 0
    L_builtin eventfd write "$ev" 7
    L_builtin eventfd apply "$ev" 0
    # counter should be 0; read should block, so read with poll instead
    L_unittest_cmd -cjN ! L_builtin eventfd read "$ev"

    # default apply value (0) also drains
    L_builtin eventfd write "$ev" 9
    L_builtin eventfd apply "$ev"
    L_unittest_cmd -cjN ! L_builtin eventfd read "$ev"

    L_builtin close "$ev"
}

_L_test_eventfd_help_short() {
    local out rc
    out="$(L_builtin eventfd -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
}

_L_test_eventfd_help_long() {
    local out rc
    out="$(L_builtin eventfd --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_eventfd_help_create() {
    local out rc
    out="$(L_builtin eventfd create -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin eventfd create: usage: create"
}

_L_test_eventfd_help_apply() {
    local out rc
    out="$(L_builtin eventfd apply -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin eventfd apply: usage: apply"
}

_L_test_eventfd_unknown() {
    L_unittest_cmd -cjN ! L_builtin eventfd bogus
}
