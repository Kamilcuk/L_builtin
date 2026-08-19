_L_test_fcntl_getfl_getfd() {
    local -a p=()
    L_builtin pipe p
    local fd=${p[0]}

    # getfl on a pipe read end (O_RDONLY = 0)
    L_unittest_checkexit 0 L_builtin fcntl getfl "$fd"

    # getfd on a valid fd (no fd flags set by default)
    L_unittest_checkexit 0 L_builtin fcntl getfd "$fd"

    # Store getfl result in a variable (should be non-zero after we set flags)
    L_builtin fcntl getfl -v fl_result "$fd"
    L_unittest_ne "$fl_result" ""

    # Store getfd result in a variable (FD_CLOEXEC is set by ensure_high_fd)
    L_builtin fcntl getfd -v fd_result "$fd"
    L_unittest_eq "$fd_result" "1"

    exec {p[0]}<&- 2>/dev/null || true
    exec {p[1]}>&- 2>/dev/null || true
}

_L_test_fcntl_setfl() {
    local -a p=()
    L_builtin pipe p
    local fd=${p[0]}

    # Set nonblock and append
    L_unittest_checkexit 0 L_builtin fcntl setfl "$fd" nonblock,append

    # Verify flags were set (should be non-zero)
    L_builtin fcntl getfl -v fl "$fd"
    L_unittest_ne "$fl" "0"

    # Clear all status flags
    L_unittest_checkexit 0 L_builtin fcntl setfl "$fd" ''

    # After clearing, only access mode remains (O_RDONLY=0 for pipe read end)
    L_builtin fcntl getfl -v fl "$fd"
    L_unittest_eq "$fl" "0"

    exec {p[0]}<&- 2>/dev/null || true
    exec {p[1]}>&- 2>/dev/null || true
}

_L_test_fcntl_setfd() {
    local -a p=()
    L_builtin pipe p
    local fd=${p[0]}

    # Set close-on-exec
    L_unittest_checkexit 0 L_builtin fcntl setfd "$fd" cloexec

    # Verify (FD_CLOEXEC = 1)
    L_builtin fcntl getfd -v ff "$fd"
    L_unittest_eq "$ff" "1"

    # Clear close-on-exec
    L_unittest_checkexit 0 L_builtin fcntl setfd "$fd" ''

    # Verify cleared
    L_builtin fcntl getfd -v ff "$fd"
    L_unittest_eq "$ff" "0"

    exec {p[0]}<&- 2>/dev/null || true
    exec {p[1]}>&- 2>/dev/null || true
}

_L_test_fcntl_dup() {
    local -a p=()
    L_builtin pipe p
    local fd=${p[0]}
    local newfd newfd2 fd_flags

    # Basic dup (prints new fd)
    L_unittest_checkexit 0 L_builtin fcntl dup "$fd"

    # Dup with start
    L_unittest_checkexit 0 L_builtin fcntl dup "$fd" 0

    # Dup with variable output
    L_builtin fcntl dup -v newfd "$fd"
    L_unittest_ne "$newfd" ""

    # The new fd should be different from the original
    L_unittest_ne "$newfd" "$fd"

    # Dup with cloexec flag
    L_builtin fcntl dup -c -v newfd2 "$fd"
    L_unittest_ne "$newfd2" ""

    # Verify cloexec was set on the dup'd fd
    L_builtin fcntl getfd -v fd_flags "$newfd2"
    L_unittest_eq "$fd_flags" "1"

    exec {p[0]}<&- 2>/dev/null || true
    exec {p[1]}>&- 2>/dev/null || true
    exec {newfd}>&- 2>/dev/null || true
    exec {newfd2}>&- 2>/dev/null || true
}

_L_test_fcntl_unknown() {
    local -a p=()
    L_builtin pipe p
    local fd=${p[0]}

    # Unknown subcommand
    L_unittest_checkexit 2 L_builtin fcntl bogus "$fd"

    # Unknown flag name
    L_unittest_checkexit 2 L_builtin fcntl setfl "$fd" badflag
    L_unittest_checkexit 2 L_builtin fcntl setfd "$fd" badflag

    exec {p[0]}<&- 2>/dev/null || true
    exec {p[1]}>&- 2>/dev/null || true
}
