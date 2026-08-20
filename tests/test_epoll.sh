# Tests for the `L_builtin epoll` subcommand group: epoll_create1/ctl/wait/close.

_L_test_epoll_create_close() {
    L_builtin epoll create ep
    [[ "$ep" =~ ^[0-9]+$ ]] || L_unittest_fail "ep is not an integer: '$ep'"
    L_builtin close "$ep"
}

_L_test_epoll_wait_readable_sparse() {
    L_builtin epoll create ep
    local -a p=()
    L_builtin pipe p
    # Nothing registered yet -> no readiness.
    local -a ready=()
    L_builtin epoll wait -t 0.05 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "0"
    # Register the pipe read end for reads (default token 'r').
    L_builtin epoll add "$ep" "${p[0]}"
    # Not readable yet: no data written.
    L_builtin epoll wait -t 0.05 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "0"
    # Write data -> the read end becomes readable.
    printf 'hello' >&"${p[1]}"
    L_builtin epoll wait -t 0.2 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "1"
    # Sparse array: index is the fd, value is the readiness tokens.
    L_unittest_eq "${ready[${p[0]}]}" "r"
    L_unittest_eq "${!ready[@]}" "${p[0]}"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
    L_builtin epoll del "$ep" "${p[0]}"
    L_builtin close "$ep"
}

_L_test_epoll_wait_writable_sparse() {
    L_builtin epoll create ep
    local -a p=()
    L_builtin pipe p
    # The write end of a pipe is writable while the buffer has space.
    L_builtin epoll add "$ep" "${p[1]}" w
    local -a ready=()
    L_builtin epoll wait -t 0.2 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "1"
    L_unittest_eq "${ready[${p[1]}]}" "w"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
    L_builtin epoll del "$ep" "${p[1]}"
    L_builtin close "$ep"
}

_L_test_epoll_timeout_empty() {
    L_builtin epoll create ep
    local -a wait=()
    L_builtin epoll wait -t 0.05 -v wait "$ep"
    L_unittest_eq "${#wait[@]}" "0"
    L_builtin close "$ep"
}

_L_test_epoll_mod_and_del() {
    L_builtin epoll create ep
    local -a p=()
    L_builtin pipe p
    L_builtin epoll add "$ep" "${p[0]}" r
    # Switch interest to writes (a pipe read end is never writable).
    L_builtin epoll mod "$ep" "${p[0]}" w
    local -a ready=()
    L_builtin epoll wait -t 0.05 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "0"
    L_builtin epoll del "$ep" "${p[0]}"
    # After del, data written no longer makes the fd report ready.
    printf 'hello' >&"${p[1]}"
    L_builtin epoll wait -t 0.1 -v ready "$ep"
    L_unittest_eq "${#ready[@]}" "0"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
    L_builtin close "$ep"
}

_L_test_epoll_poll_interop() {
    # epoll-produced fds compose with the `poll` subcommand.
    L_builtin epoll create ep
    local -a p=()
    L_builtin pipe p
    L_builtin epoll add "$ep" "${p[0]}" rt
    printf 'data' >&"${p[1]}"
    local -a results=()
    L_builtin poll -t 0.2 -v results "$ep:r"
    L_unittest_eq "${#results[@]}" "1"
    L_unittest_eq "${results[$ep]}" "r"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
    L_builtin epoll del "$ep" "${p[0]}"
    L_builtin close "$ep"
}

_L_test_epoll_help_short() {
    local out rc
    out="$(L_builtin epoll -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
}

_L_test_epoll_help_long() {
    local out rc
    out="$(L_builtin epoll --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_epoll_help_create() {
    local out rc
    out="$(L_builtin epoll create -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin epoll create: usage: create"
}

_L_test_epoll_unknown() {
    L_unittest_checkexit 2 L_builtin epoll bogus
}
