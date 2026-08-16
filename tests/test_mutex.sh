# Tests for the `L_builtin mutex` subcommand: a process-shared lock backed by
# shared memory.

# Deterministic, non-blocking check of acquire/release logic within a single
# process (no fork, so it can never hang the suite).
_L_test_mutex_poll() {
    L_builtin mutex create m
    [[ "$m" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$m'"

    # Acquire (free -> held).
    L_unittest_cmd -c L_builtin mutex lock -n "$m"

    # Already held -> non-blocking lock must fail.
    L_unittest_cmd -cjN ! L_builtin mutex lock -n "$m"

    # Release.
    L_unittest_cmd -c L_builtin mutex unlock "$m"

    # Free again -> acquire succeeds.
    L_unittest_cmd -c L_builtin mutex lock -n "$m"

    # Release (balanced).
    L_unittest_cmd -c L_builtin mutex unlock "$m"

    # Unbalanced unlock (we no longer hold it) must fail.
    L_unittest_cmd -cjN ! L_builtin mutex unlock "$m"

    L_builtin mutex close "$m"
}

# The mutex is shared across a forked process (anonymous shared memory): the
# parent holds the lock, a background child blocks on it, and is released when
# the parent unlocks. A safety killer reaps the child if anything goes wrong.
_L_test_mutex_cross_process() {
    L_builtin mutex create m
    L_builtin mutex lock "$m"
    local tmpf="$(mktemp)"
    ( L_builtin mutex lock "$m"; echo CHILD_DONE > "$tmpf"; L_builtin mutex unlock "$m" ) &
    local cpid=$!

    ( sleep 10; kill "$cpid" 2>/dev/null ) &
    local killer=$!
    sleep 0.1
    L_builtin mutex unlock "$m"
    wait "$cpid" 2>/dev/null
    kill "$killer" 2>/dev/null

    local out="$(<"$tmpf")"
    rm -f "$tmpf"
    L_builtin mutex destroy "$m"
    L_unittest_eq "$out" CHILD_DONE
}

# Named mutex: create with -n, open it, then destroy (which unlinks the
# shared-memory object globally). Opening a non-existent named mutex must fail.
_L_test_mutex_named() {
    L_builtin mutex create -n /mutex_test_named m
    L_builtin mutex open w /mutex_test_named
    L_unittest_cmd -c L_builtin mutex lock -n "$w"
    L_unittest_cmd -c L_builtin mutex unlock "$w"
    L_builtin mutex close "$w"
    L_builtin mutex destroy "$m"

    L_unittest_cmd -cjN ! L_builtin mutex open w /mutex_does_not_exist
}

# Robust mutex: a forked child acquires the lock and then exits normally without
# unlocking. The kernel marks the mutex owner-dead, so the parent's next lock
# must recover (EOWNERDEAD -> consistent) instead of deadlocking. The child
# exits normally (no _exit(1)/kill) - this exercises the real "terminate" path.
_L_test_mutex_robust() {
    L_builtin mutex create -r m
    ( L_builtin mutex lock -n "$m" )
    L_unittest_cmd -c L_builtin mutex lock -n "$m"
    L_builtin mutex unlock "$m"
    L_builtin mutex destroy "$m"
}

# unlock -a releases every mutex this process currently holds at once.
_L_test_mutex_unlock_all() {
    L_builtin mutex create a
    L_builtin mutex create b
    L_unittest_cmd -c L_builtin mutex lock -n "$a"
    L_unittest_cmd -c L_builtin mutex lock -n "$b"

    L_unittest_cmd -c L_builtin mutex unlock -a

    # Both are free again -> re-acquire succeeds.
    L_unittest_cmd -c L_builtin mutex lock -n "$a"
    L_unittest_cmd -c L_builtin mutex lock -n "$b"

    L_builtin mutex unlock -a
    L_builtin mutex close "$a"
    L_builtin mutex close "$b"
}

# unlock -a with nothing held is a safe no-op.
_L_test_mutex_unlock_all_empty() {
    L_builtin mutex create a
    L_unittest_cmd -c L_builtin mutex unlock -a
    L_builtin mutex close "$a"
}


_L_test_mutex_usage() {
    # No subcommand at all -> usage error.
    L_unittest_cmd -cjN ! L_builtin mutex

    # Unknown subcommand.
    L_unittest_cmd -cjN ! L_builtin mutex bogus

    # Missing MUTEX.
    L_unittest_cmd -cjN ! L_builtin mutex lock

    # Missing NAME.
    L_unittest_cmd -cjN ! L_builtin mutex open m

    # unlock with neither a handle nor -a -> usage error.
    L_unittest_cmd -cjN ! L_builtin mutex unlock
}

_L_test_mutex_help_short() {
    L_unittest_cmd -jr "usage" L_builtin mutex -h
}

_L_test_mutex_help_create() {
    L_unittest_cmd -jr "usage" L_builtin mutex create -h
}

_L_test_mutex_help_open() {
    L_unittest_cmd -jr "usage" L_builtin mutex open -h
}

_L_test_mutex_help_lock() {
    L_unittest_cmd -jr "usage" L_builtin mutex lock -h
}

_L_test_mutex_help_unlock() {
    L_unittest_cmd -jr "usage" L_builtin mutex unlock -h
}

_L_test_mutex_help_close() {
    L_unittest_cmd -jr "usage" L_builtin mutex close -h
}

_L_test_mutex_help_destroy() {
    L_unittest_cmd -jr "usage" L_builtin mutex destroy -h
}
