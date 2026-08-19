# Tests for the `L_builtin timerfd` subcommand group: timerfd(2) creation and
# re-arming via create/set.

_L_test_timerfd_create_and_poll() {
    # Create a timer that fires after 0.3s, then poll for readability.
    L_builtin timerfd create -s 0.3 tf
    [[ "$tf" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$tf'"

    # Should not be readable yet (timer not expired after 0.1s).
    local -a results=()
    L_builtin poll -t 0.1 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "0"

    # After ~0.3s the fd becomes readable (timer expired).
    L_builtin poll -t 0.5 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "1"

    # Drain the expiration notification so the fd is not still readable.
    L_builtin lseek "$tf" 0 2>/dev/null || :

    L_builtin close "$tf"
}

_L_test_timerfd_create_defaults() {
    # No -s means no arming (counter not set); fd is created and stored in var.
    L_builtin timerfd create tf
    [[ "$tf" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$tf'"
    L_builtin close "$tf"
}

_L_test_timerfd_set_change_expiry() {
    # Create a 0.5s timer, then set a 0.1s expiry and verify it fires sooner.
    L_builtin timerfd create -s 0.5 tf
    # Should not fire within 0.2s (old 0.5s hasn't elapsed).
    local -a results=()
    L_builtin poll -t 0.2 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "0"
    # Set a 0.1s expiry.
    L_builtin timerfd set -s 0.1 "$tf"
    # Should fire within 0.3s now.
    L_builtin poll -t 0.3 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "1"
    L_builtin close "$tf"
}

_L_test_timerfd_set_interval() {
    # Create a one-shot 0.1s timer, then make it periodic with 0.1s interval.
    L_builtin timerfd create -s 0.1 tf
    # First expiration.
    local -a results=()
    L_builtin poll -t 0.3 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "1"

    # Set a 0.1s interval and re-arm with 0.1s initial expiry.
    L_builtin timerfd set -s 0.1 -i 0.1 "$tf"
    L_builtin lseek "$tf" 0 2>/dev/null || :
    L_builtin poll -t 0.3 -v results "$tf:r"
    L_unittest_eq "${#results[@]}" "1"
    L_builtin close "$tf"
}

_L_test_timerfd_set_requires_arg() {
    L_builtin timerfd create -s 0.5 tf
    # Neither -s nor -i -> usage error (EX_USAGE = 2).
    L_unittest_checkexit 2 L_builtin timerfd set "$tf"
    L_builtin close "$tf"
}

_L_test_timerfd_help_short() {
    local out rc
    out="$(L_builtin timerfd -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
}

_L_test_timerfd_help_long() {
    local out rc
    out="$(L_builtin timerfd --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_timerfd_help_create() {
    local out rc
    out="$(L_builtin timerfd create -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin timerfd create: usage: create"
}

_L_test_timerfd_help_set() {
    local out rc
    out="$(L_builtin timerfd set -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin timerfd set: usage: set"
}

_L_test_timerfd_unknown() {
    L_unittest_cmd -cjN ! L_builtin timerfd bogus
}
