# Tests for the `L_builtin shm` subcommand: bash array variables whose value
# is shared across processes. The database is selected by `-s NAME` (POSIX
# shared memory), `-n NAME` (anonymous in-memory mapping), `-f PATH` (a regular
# file), or the default in-memory mapping named DEFAULT when no flag is given.

_L_test_shm_add_and_read() {
    local shm="SHMTEST_ADD"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    L_builtin shm rm -n "$shm"
}

_L_test_shm_single_index_rmw() {
    local shm="SHMTEST_RMW"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" V
    V=(1 2 3 4 5)
    # A single-index assignment must not clobber the other elements, because the
    # setter read-modify-writes the whole array in the shared database.
    V[0]=X
    V[2]=Y
    L_unittest_eq "${V[*]}" "X 2 Y 4 5"
    L_builtin shm rm -n "$shm"
}

_L_test_shm_info() {
    local shm="SHMTEST_INFO"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" V
    V=(p q r)
    # Verify the data persisted in the database via a forked read-back (this is
    # what `info` reads), rather than asserting on bash's printed array format.
    local tmpf
    tmpf="$(mktemp)"
    bg() {
        sleep 0.3
        echo "V=${V[*]}" > "$1"
    }
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(<"$tmpf")" "V=p q r"
    rm -f "$tmpf"
    # `info` lists the variable (format-independent name check).
    local out
    out="$(L_builtin shm info -n "$shm")"
    L_unittest_contains "$out" "V="
    L_builtin shm rm -n "$shm"
}

# `unbind` removes a variable from this shell's registry (and unbinds the bash
# variable) but leaves the database (and other variables) intact.
_L_test_shm_unbind_var() {
    local shm="SHMTEST_UNBIND"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" A
    L_builtin shm add -n "$shm" B
    A=(1 2)
    B=(x y)
    L_builtin shm unbind -n "$shm" A
    # The unbound variable is now empty.
    L_unittest_eq "${A[*]}" ""
    # The other variable remains bound and its data is intact.
    L_unittest_eq "${B[*]}" "x y"
    L_unittest_eq "${B[1]}" "y"
    L_builtin shm rm -n "$shm"
}

# `rm` deletes the whole database and unbinds every bound variable.
_L_test_shm_remove() {
    local shm="SHMTEST_REMOVE"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" REM_A
    L_builtin shm add -n "$shm" REM_B
    REM_A=(1 2)
    REM_B=(x y)
    L_builtin shm rm -n "$shm"
    L_unittest_eq "${REM_A[*]}" ""
    L_unittest_eq "${REM_B[*]}" ""
    L_builtin shm rm -n "$shm" 2>/dev/null || :
}

# The variable is shared with a background process: the parent writes after the
# child is launched, and the child (forked, mapping the same in-memory db)
# observes the parent's update.
_L_test_shm_cross_process() {
    local shm="SHMTEST_XPROC"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" B
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
    L_builtin shm rm -n "$shm"
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

_L_test_shm_help_remove() {
    local out rc
    out="$(L_builtin shm rm -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm rm: usage: rm"
    L_unittest_contains "$out" "Examples"
}

_L_test_shm_help_unbind() {
    local out rc
    out="$(L_builtin shm unbind -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm unbind: usage: unbind"
    L_unittest_contains "$out" "Examples"
}

_L_test_shm_help_info() {
    local out rc
    out="$(L_builtin shm info -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm info: usage: info"
    L_unittest_contains "$out" "Examples"
}

_L_test_shm_help_ls() {
    local out rc
    out="$(L_builtin shm ls -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm ls: usage: ls"
}

_L_test_shm_assoc_add_and_read() {
    local shm="SHMTEST_ASSOC_ADD"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -A -n "$shm" V
    V=( [foo]=bar [baz]=qux )
    L_unittest_eq "${V[foo]}" "bar"
    L_unittest_eq "${V[baz]}" "qux"
    local keys
    keys="$(printf '%s\n' "${!V[@]}" | sort)"
    L_unittest_eq "$keys" $'baz\nfoo'
    L_builtin shm rm -n "$shm"
}

_L_test_shm_assoc_single_key_rmw() {
    local shm="SHMTEST_ASSOC_RMW"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -A -n "$shm" V
    V=( [a]=1 [b]=2 [c]=3 )
    # A single-key assignment must not clobber the other elements
    V[a]=X
    V[c]=Y
    L_unittest_eq "${V[a]}" "X"
    L_unittest_eq "${V[b]}" "2"
    L_unittest_eq "${V[c]}" "Y"
    L_builtin shm rm -n "$shm"
}

_L_test_shm_assoc_info() {
    local shm="SHMTEST_ASSOC_INFO"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -A -n "$shm" V
    V=( [p]=x [q]=y [r]=z )
    local out
    out="$(L_builtin shm info -n "$shm")"
    L_unittest_contains "$out" '["p"]="x"'
    L_unittest_contains "$out" '["q"]="y"'
    L_unittest_contains "$out" '["r"]="z"'
    L_builtin shm rm -n "$shm"
}

_L_test_shm_assoc_cross_process() {
    local shm="SHMTEST_ASSOC_XPROC"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -A -n "$shm" V
    V=( [a]=1 [b]=2 [c]=3 )

    local tmpf
    tmpf="$(mktemp)"
    bg() {
        sleep 0.5
        echo "bg: ${V[b]}" > "$1"
    }
    local pid=""
    L_with_process_into pid bg "$tmpf"

    V[b]=CHANGED
    wait "$pid"

    local out
    out="$(<"$tmpf")"
    L_unittest_eq "$out" "bg: CHANGED"
    rm -f "$tmpf"
    L_builtin shm rm -n "$shm"
}

_L_test_shm_mixed_indexed_and_assoc() {
    local shm="SHMTEST_MIXED"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" IDX
    L_builtin shm add -A -n "$shm" ASSOC
    IDX=(a b c)
    ASSOC=( [foo]=bar [baz]=qux )
    # Both an indexed and an associative array live in the SAME database: a
    # forked child reads both back correctly. We verify the actual variable
    # values, not a printed (bash-format) representation.
    local tmpf
    tmpf="$(mktemp)"
    bg() {
        sleep 0.3
        echo "IDX=${IDX[*]}" > "$1"
        echo "FOO=${ASSOC[foo]}" >> "$1"
        echo "BAZ=${ASSOC[baz]}" >> "$1"
    }
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    local out
    out="$(<"$tmpf")"
    L_unittest_eq "$(printf '%s\n' "$out" | sed -n '1p')" "IDX=a b c"
    L_unittest_eq "$(printf '%s\n' "$out" | sed -n '2p')" "FOO=bar"
    L_unittest_eq "$(printf '%s\n' "$out" | sed -n '3p')" "BAZ=qux"
    rm -f "$tmpf"
    L_builtin shm rm -n "$shm"
}

# POSIX shared memory (-s): a name-based database shared across unrelated
# processes. Verified here within a forked child.
_L_test_shm_posix_add_and_read() {
    local shm="SHMTEST_POSIX"
    L_builtin shm rm -s "$shm" 2>/dev/null || :
    L_builtin shm add -s "$shm" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    # The POSIX shm object exists as /dev/shm/<name>.
    L_unittest_eq "$(ls -1 /dev/shm/ 2>/dev/null | grep -c "^SHMTEST_POSIX$")" "1"
    L_builtin shm rm -s "$shm"
    L_unittest_eq "$(ls -1 /dev/shm/ 2>/dev/null | grep -c "^SHMTEST_POSIX$")" "0"
}

# File-backed (-f) database at an arbitrary path.
_L_test_shm_file_add_and_read() {
    local path
    path="$(mktemp)"
    L_builtin shm rm -f "$path" 2>/dev/null || :
    L_builtin shm add -f "$path" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    L_builtin shm rm -f "$path"
}

# The default database (no flag) is the in-memory mapping named DEFAULT.
_L_test_shm_default_add_and_read() {
    L_builtin shm rm 2>/dev/null || :
    L_builtin shm add DEFV
    DEFV=(a b c)
    L_unittest_eq "${DEFV[*]}" "a b c"
    L_unittest_eq "${DEFV[1]}" "b"
    L_builtin shm rm
}

# `ls` without arguments lists every database this session knows about together
# with the variables bound to each.
_L_test_shm_ls_all() {
    local shm1="SHMTEST_LS1"
    local shm2="SHMTEST_LS2"
    L_builtin shm rm -n "$shm1" 2>/dev/null || :
    L_builtin shm rm -n "$shm2" 2>/dev/null || :
    L_builtin shm add -n "$shm1" A
    L_builtin shm add -n "$shm2" B
    local out
    out="$(L_builtin shm ls)"
    L_unittest_contains "$out" "DB memfd:$shm1: A"
    L_unittest_contains "$out" "DB memfd:$shm2: B"
    L_builtin shm rm -n "$shm1"
    L_builtin shm rm -n "$shm2"
}

# `ls -n SHM_NAME` lists only the variables bound to SHM_NAME in this session's
# REGISTRY (not every entry another process may have written to the database).
_L_test_shm_ls_named() {
    local shm="SHMTEST_LSNAMED"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm add -n "$shm" A
    L_builtin shm add -n "$shm" B
    local out
    out="$(L_builtin shm ls -n "$shm")"
    L_unittest_contains "$out" "A"
    L_unittest_contains "$out" "B"
    L_builtin shm rm -n "$shm"
}

# `ls -s NAME` matches only the database created with -s; a memfd database of
# the same name is a different backing and must not be listed.
_L_test_shm_ls_named_distinguishes_backing() {
    L_builtin shm add -n MEMDISTINCT MEMDISTINCT
    local out
    out="$(L_builtin shm ls -s MEMDISTINCT)"
    L_unittest_eq "$out" ""
    L_builtin shm rm -n MEMDISTINCT
}
