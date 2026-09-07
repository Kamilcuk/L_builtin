# Tests for the `L_builtin sig` subcommand: managing the shell's signal mask.
#
# Note: bash signal masks are per-process and are NOT inherited by subshells,
# so we cannot use $(...) to capture sig list output (it runs in a child).
# We use -v VAR which writes to a variable in the *current* shell.

_L_test_sig_list_empty() {
    # Sanity check: when nothing is blocked, sig list -v VAR gives an empty array.
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "0"
}

_L_test_sig_list_blocked_int() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block INT
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGINT"
    L_builtin sig unblock all 2>/dev/null || :
}

_L_test_sig_list_capture_to_var() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block USR1 USR2
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "2"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_unittest_eq "${blocked[2]}" "SIGUSR2"
    L_builtin sig unblock all 2>/dev/null || :
}

# --- sig block / sig unblock ---

_L_test_sig_block_unblock() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block INT
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGINT"
    L_builtin sig unblock INT
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "0"
}

_L_test_sig_block_multiple() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block INT TERM
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "2"
    L_builtin sig unblock all 2>/dev/null || :
}

_L_test_sig_unblock_multiple() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block USR1 USR2
    L_builtin sig unblock USR1
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR2"
    L_builtin sig unblock all 2>/dev/null || :
}

# --- Signal delivery (signal raised while blocked vs unblocked) ---

_L_test_sig_blocked_signal_not_delivered() {
    # When a signal is blocked, raising it should NOT fire the trap.
    local CAUGHT=0
    trap 'CAUGHT=1' USR1

    L_builtin sig block USR1
    L_raise -USR1
    # Give bash a chance to process the signal if it were unblocked.
    sleep 0.05
    L_unittest_eq "$CAUGHT" "0" "USR1 trap should not fire while blocked"

    L_builtin sig unblock USR1
}

_L_test_sig_unblock_pending_signal() {
    # Block USR1, raise it (becomes pending), then unblock - trap should fire.
    local CAUGHT=0
    trap 'CAUGHT=1' USR1

    L_builtin sig block USR1
    L_raise -USR1
    L_unittest_eq "$CAUGHT" "0" "trap should not fire while signal is blocked"

    L_builtin sig unblock USR1
    # Trigger pending-trap processing: any builtin call will do.
    :
    L_unittest_eq "$CAUGHT" "1" "trap should fire after unblocking pending signal"
}

_L_test_sig_run_unblocks_for_command() {
    # Block SIGUSR1 in the parent, then run a command with USR1 unblocked.
    # The parent's mask must remain unchanged after the run.
    local CAUGHT=0
    trap 'CAUGHT=1' USR1

    L_builtin sig block USR1
    L_builtin sig run USR1 -- true
    # The parent's mask must remain unchanged.
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_builtin sig unblock USR1
}

# --- sig run ---

_L_test_sig_run_simple() {
    L_builtin sig unblock all 2>/dev/null || :
    local out
    out="$(L_builtin sig run INT -- echo RUNS)"
    L_unittest_eq "$out" "RUNS"
}

_L_test_sig_run_multiple_sigs() {
    L_builtin sig unblock all 2>/dev/null || :
    local out
    out="$(L_builtin sig run INT TERM -- echo "INT and TERM")"
    L_unittest_eq "$out" "INT and TERM"
}

_L_test_sig_run_does_not_modify_parent_mask() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block USR1
    # Run with a different signal unblocked - parent's USR1 block must remain.
    L_builtin sig run USR2 -- true
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_builtin sig unblock all 2>/dev/null || :
}

_L_test_sig_run_exit_status() {
    L_builtin sig unblock all 2>/dev/null || :
    # `false` returns 1
    L_unittest_cmd -ce 1 L_builtin sig run INT -- false
    # `true` returns 0
    L_unittest_cmd -c L_builtin sig run INT -- true
}

_L_test_sig_run_command_with_args() {
    L_builtin sig unblock all 2>/dev/null || :
    local out
    out="$(L_builtin sig run INT -- echo a b c d)"
    L_unittest_eq "$out" "a b c d"
}

# --- Signal spec formats ---

_L_test_sig_numeric_signal_spec() {
    L_builtin sig unblock all 2>/dev/null || :
    # SIGUSR1 is signal 10 on Linux
    L_builtin sig block 10
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_builtin sig unblock all 2>/dev/null || :
}

_L_test_sig_prefix_optional() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block SIGUSR1
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_builtin sig unblock USR1 2>/dev/null || :
    L_builtin sig block USR1
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "1"
    L_unittest_eq "${blocked[1]}" "SIGUSR1"
    L_builtin sig unblock all 2>/dev/null || :
}

_L_test_sig_invalid_signal() {
    L_builtin sig unblock all 2>/dev/null || :
    L_unittest_cmd -cjN ! L_builtin sig block BOGUS
    L_unittest_cmd -cjN ! L_builtin sig unblock NOT_A_SIGNAL
    L_unittest_cmd -cjN ! L_builtin sig run NOT_A_SIGNAL -- echo x
}

_L_test_sig_all_specifier() {
    L_builtin sig unblock all 2>/dev/null || :
    L_builtin sig block all
    L_builtin sig list -v blocked
    (( blocked_count = ${#blocked[@]} ))
    # Bash defines at least 32 standard signals; expect a large count.
    (( blocked_count > 30 )) || L_unittest_failure "expected >30 blocked, got $blocked_count"
    L_builtin sig unblock all
    L_builtin sig list -v blocked
    L_unittest_eq "${#blocked[@]}" "0"
}

# --- Usage errors ---

_L_test_sig_missing_subcommand() {
    L_unittest_cmd -cjN ! L_builtin sig
}

_L_test_sig_missing_sigs_block() {
    L_unittest_cmd -cjN ! L_builtin sig block
}

_L_test_sig_missing_sigs_unblock() {
    L_unittest_cmd -cjN ! L_builtin sig unblock
}

_L_test_sig_run_missing_separator() {
    L_unittest_cmd -cjN ! L_builtin sig run INT echo hello
    L_unittest_cmd -cjN ! L_builtin sig run INT
}

_L_test_sig_run_no_signals_before_separator() {
    L_unittest_cmd -cjN ! L_builtin sig run -- echo hello
}

_L_test_sig_run_no_command_after_separator() {
    L_unittest_cmd -cjN ! L_builtin sig run INT --
}

# --- Help ---

_L_test_sig_help_short() {
    local out rc
    out="$(L_builtin sig -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "list"
    L_unittest_contains "$out" "block"
    L_unittest_contains "$out" "unblock"
    L_unittest_contains "$out" "run"
}

_L_test_sig_help_long() {
    local out rc
    out="$(L_builtin sig --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_sig_help_subcommand() {
    local out rc
    out="$(L_builtin sig list -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
    L_unittest_contains "$out" "Examples"
}