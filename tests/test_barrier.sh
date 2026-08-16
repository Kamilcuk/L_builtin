# Tests for the `L_builtin barrier` subcommand: process synchronization
# barriers backed by shared memory.

# Deterministic, non-blocking check of the core arrival/satisfaction/reset logic
# within a single process (no fork, so it can never hang the suite).
_L_test_barrier_poll() {
    L_builtin barrier create h 2
    [[ "$h" =~ ^[0-9]+$ ]] || L_unittest_fail "handle is not an integer: '$h'"

    # First non-blocking arrival: 1/2, not satisfied yet.
    L_unittest_cmd -ce 1 L_builtin barrier wait "$h" -n

    # Second arrival: 2/2, satisfied.
    L_unittest_cmd -c L_builtin barrier wait "$h" -n

    # Sticky: still satisfied after the round is released.
    L_unittest_cmd -c L_builtin barrier wait "$h" -n

    # Reset clears the satisfied state.
    L_builtin barrier reset "$h"
    L_unittest_cmd -ce 1 L_builtin barrier wait "$h" -n

    L_builtin barrier close "$h"
}

# The barrier is shared across a forked process (anonymous shared memory): a
# background child blocks on wait and is released when the parent arrives. The
# parent arrives via a bounded non-blocking poll so this test cannot hang the
# suite; a safety killer reaps the child if anything goes wrong.
_L_test_barrier_cross_process() {
    L_builtin barrier create h 2
    local tmpf="$(mktemp)"
    ( L_builtin barrier wait "$h"; echo CHILD_DONE > "$tmpf" ) &
    local cpid=$!

    local satisfied=0
    for ((i = 0; i < 100; i++)); do
        L_builtin barrier wait "$h" -n && { satisfied=1; break; }
        sleep 0.05
    done

    # Safety net so a broken barrier cannot hang the test run.
    ( sleep 10; kill "$cpid" 2>/dev/null ) &
    local killer=$!
    wait "$cpid" 2>/dev/null
    kill "$killer" 2>/dev/null

    local out="$(<"$tmpf")"
    rm -f "$tmpf"
    L_builtin barrier close "$h"
    L_unittest_eq "$out" CHILD_DONE
    L_unittest_eq "$satisfied" 1
}

# Named barrier: create with -n, then destroy (which unlinks the shared-memory
# object globally). Also a missing open must fail.
_L_test_barrier_named() {
    L_builtin barrier create -n /barriertest_named h 2
    L_unittest_cmd -c L_builtin barrier destroy "$h"

    # Opening a non-existent named barrier must fail.
    L_unittest_cmd -cjN ! L_builtin barrier open w /barriertest_does_not_exist
}

_L_test_barrier_usage() {
    # No subcommand at all -> usage error.
    L_unittest_cmd -cjN ! L_builtin barrier

    # Unknown subcommand.
    L_unittest_cmd -cjN ! L_builtin barrier bogus

    # Missing count.
    L_unittest_cmd -cjN ! L_builtin barrier create h

    # Count must be >= 1.
    L_unittest_cmd -cjN ! L_builtin barrier create h 0
}

_L_test_barrier_help_short() {
    local out rc
    out="$(L_builtin barrier -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
}

_L_test_barrier_help_long() {
    local out rc
    out="$(L_builtin barrier --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_barrier_help_create() {
    local out rc
    out="$(L_builtin barrier create -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}

_L_test_barrier_help_wait() {
    local out rc
    out="$(L_builtin barrier wait -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}

_L_test_barrier_help_open() {
    local out rc
    out="$(L_builtin barrier open -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}

_L_test_barrier_help_close() {
    local out rc
    out="$(L_builtin barrier close -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}

_L_test_barrier_help_reset() {
    local out rc
    out="$(L_builtin barrier reset -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}

_L_test_barrier_help_destroy() {
    local out rc
    out="$(L_builtin barrier destroy -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}
