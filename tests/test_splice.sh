# Tests for the `L_builtin splice` subcommand: zero-copy fd-to-fd moves.

_L_test_splice_pipe_to_pipe() {
    local -a p1=() p2=()
    L_builtin pipe p1
    L_builtin pipe p2
    printf 'hello splice' >&"${p1[1]}"
    L_builtin splice -v MOVED "${p1[0]}" "${p2[1]}" 12
    L_unittest_eq "$MOVED" "12"
    local got
    L_builtin read -v got "${p2[0]}" 12
    L_unittest_eq "$got" "hello splice"
    eval "exec ${p1[0]}<&-; exec ${p1[1]}>&-; exec ${p2[0]}<&-; exec ${p2[1]}>&-"
}

_L_test_splice_file_to_pipe() {
    local tmpfile
    L_with_tmpfile_into tmpfile
    printf 'fromfile' > "$tmpfile"
    exec 3<"$tmpfile"
    local -a p=()
    L_builtin pipe p
    L_builtin splice -v MOVED 3 "${p[1]}" 8
    L_unittest_eq "$MOVED" "8"
    local got
    L_builtin read -v got "${p[0]}" 8
    L_unittest_eq "$got" "fromfile"
    exec 3<&-
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_splice_flags() {
    local -a p1=() p2=()
    L_builtin pipe p1
    L_builtin pipe p2
    printf 'xyz' >&"${p1[1]}"
    L_builtin splice -v MOVED "${p1[0]}" "${p2[1]}" 3 nonblock
    L_unittest_eq "$MOVED" "3"
    local got
    L_builtin read -v got "${p2[0]}" 3
    L_unittest_eq "$got" "xyz"
    eval "exec ${p1[0]}<&-; exec ${p1[1]}>&-; exec ${p2[0]}<&-; exec ${p2[1]}>&-"
}

_L_test_splice_errors() {
    local -a p=()
    L_builtin pipe p
    L_unittest_checkexit 1 L_builtin splice -v M 999999 "${p[1]}" 5
    L_unittest_checkexit 1 L_builtin splice -v M "${p[0]}" 999999 5
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_splice_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin splice -h
}

_L_test_splice_help_long() {
    L_unittest_cmd -j -r "splice\(2\)" L_builtin splice --help
}
