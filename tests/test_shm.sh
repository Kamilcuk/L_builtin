# Tests for the `L_builtin shm` subcommand: bash array variables whose value
# is shared across processes through an LMDB database in /dev/shm.

_L_test_shm_add_and_read() {
    local shm="SHMTEST_ADD"
    L_builtin shm rm "$shm" 2>/dev/null || :
    L_builtin shm add "$shm" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    L_builtin shm rm "$shm"
}

_L_test_shm_single_index_rmw() {
    local shm="SHMTEST_RMW"
    L_builtin shm rm "$shm" 2>/dev/null || :
    L_builtin shm add "$shm" V
    V=(1 2 3 4 5)
    # A single-index assignment must not clobber the other elements, because the
    # setter read-modify-writes the whole array in LMDB.
    V[0]=X
    V[2]=Y
    L_unittest_eq "${V[*]}" "X 2 Y 4 5"
    L_builtin shm rm "$shm"
}

_L_test_shm_info() {
    local shm="SHMTEST_INFO"
    L_builtin shm rm "$shm" 2>/dev/null || :
    L_builtin shm add "$shm" V
    V=(p q r)
    local out
    out="$(L_builtin shm info "$shm")"
    L_unittest_eq "$out" 'V=([0]="p" [1]="q" [2]="r")'
    L_builtin shm rm "$shm"
}

_L_test_shm_rm_var() {
    local shm="SHMTEST_RMV"
    L_builtin shm rm "$shm" 2>/dev/null || :
    L_builtin shm add "$shm" A
    L_builtin shm add "$shm" B
    A=(1 2)
    B=(x y)
    L_builtin shm rm "$shm" A
    # The removed variable is unbound.
    L_unittest_eq "${A[*]}" ""
    # The other variable is untouched.
    local out
    out="$(L_builtin shm info "$shm")"
    L_unittest_eq "$out" 'B=([0]="x" [1]="y")'
    L_builtin shm rm "$shm"
}

# The variable is shared with a background process: the parent writes after the
# child is launched, and the child (forked, mapping the same LMDB file) observes
# the parent's update.
_L_test_shm_cross_process() {
    local shm="SHMTEST_XPROC"
    L_builtin shm rm "$shm" 2>/dev/null || :
    L_builtin shm add "$shm" B
    B=(a b c)

    local tmpf
    tmpf="$(mktemp)"
    bg() {
        sleep 0.5
        echo "bg: ${B[*]}" > "$1"
    }
    local pid=""
    L_with_process_into pid bg "$tmpf"

    B[1]=CHANGED
    wait "$pid"

    local out
    out="$(<"$tmpf")"
    L_unittest_eq "$out" "bg: a CHANGED c"
    rm -f "$tmpf"
    L_builtin shm rm "$shm"
}

_L_test_shm_help_short() {
    local out rc
    out="$(L_builtin shm -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "usage"
}

_L_test_shm_help_long() {
    local out rc
    out="$(L_builtin shm --help 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "Subcommands"
}

_L_test_shm_help_add() {
    local out rc
    out="$(L_builtin shm add -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm add: usage: add"
    L_unittest_contains "$out" "Examples"
}

_L_test_shm_help_rm() {
    local out rc
    out="$(L_builtin shm rm -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm rm: usage: rm"
    L_unittest_contains "$out" "Examples"
}

_L_test_shm_help_info() {
    local out rc
    out="$(L_builtin shm info -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm info: usage: info"
    L_unittest_contains "$out" "Examples"
}
