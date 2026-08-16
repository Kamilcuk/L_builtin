# Tests for the `L_builtin semaphore` subcommand: a process-shared counting
# semaphore backed by shared memory.

# Deterministic, non-blocking check of wait/post logic within a single process
# (no fork, so it can never hang the suite).
_L_test_semaphore_poll() {
    L_builtin semaphore create s 1
    [[ "$s" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$s'"

    # Count 1 -> wait succeeds (1 -> 0).
    L_unittest_cmd -c L_builtin semaphore wait -n "$s"

    # Count 0 -> non-blocking wait must fail.
    L_unittest_cmd -cjN ! L_builtin semaphore wait -n "$s"

    # post (0 -> 1).
    L_unittest_cmd -c L_builtin semaphore post "$s"

    # Count 1 again -> wait succeeds.
    L_unittest_cmd -c L_builtin semaphore wait -n "$s"

    L_builtin semaphore close "$s"
}

# Initial value 0: a non-blocking wait must fail until a post arrives.
_L_test_semaphore_zero() {
    L_builtin semaphore create s 0
    L_unittest_cmd -cjN ! L_builtin semaphore wait -n "$s"
    L_unittest_cmd -c L_builtin semaphore post "$s"
    L_unittest_cmd -c L_builtin semaphore wait -n "$s"
    L_builtin semaphore close "$s"
}

# The semaphore is shared across a forked process (anonymous shared memory): a
# background child blocks on wait and is released when the parent posts. A
# safety killer reaps the child if anything goes wrong.
_L_test_semaphore_cross_process() {
    L_builtin semaphore create s 0
    local tmpf="$(mktemp)"
    ( L_builtin semaphore wait "$s"; echo CHILD_DONE > "$tmpf"; L_builtin semaphore post "$s" ) &
    local cpid=$!

    ( sleep 10; kill "$cpid" 2>/dev/null ) &
    local killer=$!
    sleep 0.1
    L_builtin semaphore post "$s"
    wait "$cpid" 2>/dev/null
    kill "$killer" 2>/dev/null

    local out="$(<"$tmpf")"
    rm -f "$tmpf"
    L_builtin semaphore destroy "$s"
    L_unittest_eq "$out" CHILD_DONE
}

# Named semaphore: create with -n, open it, then destroy (which unlinks the
# kernel object globally). Opening a non-existent named semaphore must fail.
_L_test_semaphore_named() {
    L_builtin semaphore create -n /sem_test_named s 2
    L_builtin semaphore open w /sem_test_named
    L_unittest_cmd -c L_builtin semaphore wait -n "$w"
    L_unittest_cmd -c L_builtin semaphore post "$w"
    L_builtin semaphore close "$w"
    L_builtin semaphore destroy "$s"

    L_unittest_cmd -cjN ! L_builtin semaphore open w /sem_does_not_exist
}

_L_test_semaphore_usage() {
    # No subcommand at all -> usage error.
    L_unittest_cmd -cjN ! L_builtin semaphore

    # Unknown subcommand.
    L_unittest_cmd -cjN ! L_builtin semaphore bogus

    # Missing COUNT.
    L_unittest_cmd -cjN ! L_builtin semaphore create s

    # Missing SEMAPHORE.
    L_unittest_cmd -cjN ! L_builtin semaphore wait
}

_L_test_semaphore_help_short() {
    L_unittest_cmd -jr "usage" L_builtin semaphore -h
}

_L_test_semaphore_help_create() {
    L_unittest_cmd -jr "usage" L_builtin semaphore create -h
}

_L_test_semaphore_help_open() {
    L_unittest_cmd -jr "usage" L_builtin semaphore open -h
}

_L_test_semaphore_help_wait() {
    L_unittest_cmd -jr "usage" L_builtin semaphore wait -h
}

_L_test_semaphore_help_post() {
    L_unittest_cmd -jr "usage" L_builtin semaphore post -h
}

_L_test_semaphore_help_close() {
    L_unittest_cmd -jr "usage" L_builtin semaphore close -h
}

_L_test_semaphore_help_destroy() {
    L_unittest_cmd -jr "usage" L_builtin semaphore destroy -h
}
