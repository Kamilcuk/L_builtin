# Tests for the `L_builtin shm` subcommand: bash array variables whose value
# is shared across processes. The database is selected by `-s NAME` (POSIX
# shared memory), `-n NAME` (anonymous in-memory mapping), `-F PATH` (a regular
# file), or the default in-memory mapping named DEFAULT when no flag is given.

_L_test_shm_bind_and_read() {
    local shm="SHMTEST_ADD"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    L_builtin shm rm -n "$shm"
}

_L_test_shm_single_index_rmw() {
    local shm="SHMTEST_RMW"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
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
    L_builtin shm bind -n "$shm" V
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
    L_builtin shm bind -n "$shm" A
    L_builtin shm bind -n "$shm" B
    A=(1 2)
    B=(x y)
    L_builtin shm unbind A
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
    L_builtin shm bind -n "$shm" REM_A
    L_builtin shm bind -n "$shm" REM_B
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
    L_builtin shm bind -n "$shm" B
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

_L_test_shm_help_bind() {
    local out rc
    out="$(L_builtin shm bind -h 2>&1)"; rc=$?
    L_unittest_eq "$rc" 0
    L_unittest_contains "$out" "L_builtin shm bind: usage: bind"
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

_L_test_shm_assoc_bind_and_read() {
    local shm="SHMTEST_ASSOC_ADD"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -A -n "$shm" V
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
    L_builtin shm bind -A -n "$shm" V
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
    L_builtin shm bind -A -n "$shm" V
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
    L_builtin shm bind -A -n "$shm" V
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
    L_builtin shm bind -n "$shm" IDX
    L_builtin shm bind -A -n "$shm" ASSOC
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
_L_test_shm_posix_bind_and_read() {
    local shm="SHMTEST_POSIX"
    L_builtin shm rm -s "$shm" 2>/dev/null || :
    L_builtin shm bind -s "$shm" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    # The POSIX shm object exists as /dev/shm/<name>.
    L_unittest_eq "$(ls -1 /dev/shm/ 2>/dev/null | grep -c "^SHMTEST_POSIX$")" "1"
    L_builtin shm rm -s "$shm"
    L_unittest_eq "$(ls -1 /dev/shm/ 2>/dev/null | grep -c "^SHMTEST_POSIX$")" "0"
}

# File-backed (-F) database at an arbitrary path.
_L_test_shm_file_bind_and_read() {
    local path
    path="$(mktemp)"
    L_builtin shm rm -F "$path" 2>/dev/null || :
    L_builtin shm bind -F "$path" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    L_builtin shm rm -F "$path"
}

# The default database (no flag) is the in-memory mapping named DEFAULT.
_L_test_shm_default_bind_and_read() {
    L_builtin shm rm 2>/dev/null || :
    L_builtin shm bind DEFV
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
    L_builtin shm bind -n "$shm1" A
    L_builtin shm bind -n "$shm2" B
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
    L_builtin shm bind -n "$shm" A
    L_builtin shm bind -n "$shm" B
    local out
    out="$(L_builtin shm ls -n "$shm")"
    L_unittest_contains "$out" "A"
    L_unittest_contains "$out" "B"
    L_builtin shm rm -n "$shm"
}

# `ls -s NAME` matches only the database created with -s; a memfd database of
# the same name is a different backing and must not be listed.
_L_test_shm_ls_named_distinguishes_backing() {
    L_builtin shm bind -n MEMDISTINCT MEMDISTINCT
    local out
    out="$(L_builtin shm ls -s MEMDISTINCT)"
    L_unittest_eq "$out" ""
    L_builtin shm rm -n MEMDISTINCT
}

###############################################################################
# Stress / scale tests
###############################################################################
#
# These exercise the shm machinery under heavier load: large arrays, large
# element values, many variables in one database, concurrent readers, repeated
# bind/unbind cycles, and the `local` variable scoping regression fixed in the
# dynamic-variable initializer.
#
# Note on concurrency: the in-memory (-n memfd) and POSIX (-s) backings share
# their database across a fork via an inherited file descriptor / mmap. A
# pshared `pthread_rwlock_t` in the header page provides true mutual exclusion
# in this case, so concurrent *writers* are now safe (they are serialised by
# the write lock). The test below exercises this directly.

# A large indexed array round-trips intact through the shared database: a forked
# child (which inherits the binding) reads every element back. Writing is O(n^2)
# in the element count (each setter rewrites the whole blob), so keep n modest.
_L_test_shm_stress_large_indexed_array() {
    local shm="SHMTEST_STRESS_LARGE_IDX"
    local n=200
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
    local i
    for ((i = 0; i < n; i++)); do V[i]="elem_$i"; done
    # Verify via a forked read-back (the child materializes V from the db).
    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        local s=0 i
        for ((i = 0; i < n; i++)); do [[ "${V[$i]}" == "elem_$i" ]] && s=$((s + 1)); done
        echo "$s" > "$1"
    }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "$n"
    L_builtin shm rm -n "$shm"
}

# A large associative array round-trips intact: a forked child reads every key
# back and counts the matches.
_L_test_shm_stress_large_associative_array() {
    local shm="SHMTEST_STRESS_LARGE_ASSOC"
    local n=150
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -A -n "$shm" V
    local i
    for ((i = 0; i < n; i++)); do V["key_$i"]="val_$i"; done
    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        local s=0 i
        for ((i = 0; i < n; i++)); do [[ "${V[key_$i]}" == "val_$i" ]] && s=$((s + 1)); done
        echo "$s" > "$1"
    }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "$n"
    L_builtin shm rm -n "$shm"
}

# Large element values survive a write/read round-trip and are not truncated or
# cross-contaminated: a forked child verifies every value's content verbatim.
_L_test_shm_stress_large_values() {
    local shm="SHMTEST_STRESS_LARGE_VALS"
    local n=30
    local padlen=8000
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
    local pad
    pad="$(head -c "$padlen" < /dev/zero | tr '\0' 'x')"
    local i
    for ((i = 0; i < n; i++)); do V[i]="$i:${pad}"; done
    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        # Re-derive the canonical padding so length/prefix checks match the parent.
        local pad
        pad="$(head -c "$1" < /dev/zero | tr '\0' 'x')"
        local s=0 i
        for ((i = 0; i < n; i++)); do
            local got="${V[$i]}"
            local exp="$i:${pad}"
            if [[ "${#got}" == "${#exp}" && "$got" == "$exp" ]]; then s=$((s + 1)); fi
        done
        echo "$s" > "$2"
    }
    local pid=""
    L_with_process_into pid bg "$padlen" "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "$n"
    L_builtin shm rm -n "$shm"
}

# Many distinct variables coexist in one database and are all readable from a
# forked child, which writes back a count of how many it observed intact.
_L_test_shm_stress_many_variables() {
    local shm="SHMTEST_STRESS_MANY_VARS"
    local n=30
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    local i
    for ((i = 0; i < n; i++)); do
        L_builtin shm bind -n "$shm" "VAR_$i"
        eval "VAR_$i=(a b c d)"
    done
    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        local s=0 j v
        for ((j = 0; j < n; j++)); do
            eval "v=\${VAR_${j}[*]}"
            [[ "$v" == "a b c d" ]] && s=$((s + 1))
        done
        echo "$s" > "$1"
    }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "$n"
    L_builtin shm rm -n "$shm"
}

# A fan-out of concurrent readers all observe the full, consistent dataset
# after a single writer has finished populating it (read-only stress of the
# shared-lock + rkyv deserialization path). A `barrier` synchronizes the start
# so the readers issue their (read-only) database reads simultaneously rather
# than staggered.
_L_test_shm_stress_concurrent_readers() {
    local shm="SHMTEST_STRESS_READERS"
    local n=150
    local nreaders=8
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
    local i
    for ((i = 0; i < n; i++)); do V[i]="e_$i"; done
    local tmpf
    L_with_tmpfile_into tmpf
    # Barrier of (nreaders + parent): the parent arrives last to release them.
    local b
    L_builtin barrier create b $((nreaders + 1))
    L_finally L_builtin barrier destroy "$b" 2>/dev/null
    bg() {
        # All readers block here until the parent releases the barrier...
        L_builtin barrier wait "$b"
        # ...then issue their read-only database reads together.
        echo "${#V[@]}" >> "$1"
    }
    local pids=()
    for ((r = 0; r < nreaders; r++)); do
        L_with_process_into pid bg "$tmpf"
        pids+=("$pid")
    done
    # Release the readers (parent is the (nreaders+1)-th arrival).
    L_builtin barrier wait "$b"
    # Safety net so a stuck reader cannot hang the suite.
    ( sleep 8; kill "${pids[@]}" 2>/dev/null ) &
    local killer=$!
    L_finally kill "$killer" 2>/dev/null
    local p
    for p in "${pids[@]}"; do wait "$p"; done
    kill "$killer" 2>/dev/null
    local lines
    lines="$(sort -u "$tmpf")"
    L_unittest_eq "$lines" "$n"
    L_builtin shm rm -n "$shm"
}

# A fan-out of concurrent *writers* each assigns a distinct key in the same
# shared (associative) variable. With the pshared rwlock, writes are serialised
# — every key survives. Before the rwlock fix, writers clobbered each other's
# read-modify-write cycle and ~most keys were lost.
_L_test_shm_stress_concurrent_writers() {
    local shm="SHMTEST_STRESS_WRITERS"
    local nwriters=200
    L_finally L_builtin shm rm -s "$shm"
    L_builtin shm rm -s "$shm" 2>/dev/null || :
    L_builtin shm bind -A -s "$shm" V

    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        # $1 = writer index, $2 = output file
        local idx=$1 out=$2
        V[w$idx]="val_$idx"
        echo "w$idx" >> "$out"
    }

    local pids=()
    local w
    for ((w = 0; w < nwriters; w++)); do
        L_with_process_into pid bg "$w" "$tmpf"
        pids+=("$pid")
    done
    for p in "${pids[@]}"; do wait "$p" 2>/dev/null; done

    # All writers finished.
    L_unittest_eq "$(sort -u "$tmpf" | wc -l)" "$nwriters"
    # Every key must survive in the shared database.
    local count=0 i
    for ((i = 0; i < nwriters; i++)); do
        [[ "${V[w$i]}" == "val_$i" ]] && count=$((count + 1))
    done
    L_unittest_eq "$count" "$nwriters"
}

# Same concurrent-writers stress test for the -F file backend.
_L_test_shm_stress_concurrent_writers_file() {
    local shm="SHMTEST_STRESS_WRITERS_FILE"
    local nwriters=200
    L_finally L_builtin shm rm -F "$shm"
    L_builtin shm rm -F "$shm" 2>/dev/null || :
    L_builtin shm bind -A -F "$shm" V

    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        # $1 = writer index, $2 = output file
        local idx=$1 out=$2
        V[w$idx]="val_$idx"
        echo "w$idx" >> "$out"
    }

    local pids=()
    local w
    for ((w = 0; w < nwriters; w++)); do
        L_with_process_into pid bg "$w" "$tmpf"
        pids+=("$pid")
    done
    for p in "${pids[@]}"; do wait "$p" 2>/dev/null; done

    L_unittest_eq "$(sort -u "$tmpf" | wc -l)" "$nwriters"
    local count=0 i
    for ((i = 0; i < nwriters; i++)); do
        [[ "${V[w$i]}" == "val_$i" ]] && count=$((count + 1))
    done
    L_unittest_eq "$count" "$nwriters"
}

# Same concurrent-writers stress test for the default (memfd) backend.
_L_test_shm_stress_concurrent_writers_memfd() {
    local shm="SHMTEST_STRESS_WRITERS_MEMFD"
    local nwriters=200
    L_finally L_builtin shm rm -n "$shm"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -A -n "$shm" V

    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        # $1 = writer index, $2 = output file
        local idx=$1 out=$2
        V[w$idx]="val_$idx"
        echo "w$idx" >> "$out"
    }

    local pids=()
    local w
    for ((w = 0; w < nwriters; w++)); do
        L_with_process_into pid bg "$w" "$tmpf"
        pids+=("$pid")
    done
    for p in "${pids[@]}"; do wait "$p" 2>/dev/null; done

    L_unittest_eq "$(sort -u "$tmpf" | wc -l)" "$nwriters"
    local count=0 i
    for ((i = 0; i < nwriters; i++)); do
        [[ "${V[w$i]}" == "val_$i" ]] && count=$((count + 1))
    done
    L_unittest_eq "$count" "$nwriters"
}

# Same concurrent-writers stress test but for the -F file backend.
_L_test_shm_stress_concurrent_writers_file() {
    local shm="SHMTEST_STRESS_WRITERS_FILE"
    local nwriters=200
    L_builtin shm rm -F "$shm" 2>/dev/null || :
    L_builtin shm bind -A -F "$shm" V

    local tmpf
    L_with_tmpfile_into tmpf
    bg() {
        # $1 = writer index, $2 = output file
        local idx=$1 out=$2
        V[w$idx]="val_$idx"
        echo "w$idx" >> "$out"
    }

    local pids=()
    local w
    for ((w = 0; w < nwriters; w++)); do
        L_with_process_into pid bg "$w" "$tmpf"
        pids+=("$pid")
    done
    for p in "${pids[@]}"; do wait "$p" 2>/dev/null; done

    L_unittest_eq "$(sort -u "$tmpf" | wc -l)" "$nwriters"
    local count=0 i
    for ((i = 0; i < nwriters; i++)); do
        [[ "${V[w$i]}" == "val_$i" ]] && count=$((count + 1))
    done
    L_unittest_eq "$count" "$nwriters"
    L_builtin shm rm -F "$shm"
}



# Repeated add/rm cycles do not corrupt state or leak the registry: each cycle
# leaves a clean slate and the next add rebinds successfully. Reading an element
# triggers the getter, which reloads it from the shared db (not the parent's
# local cache), so a matching value proves the write persisted.
_L_test_shm_stress_repeated_bind_unbind() {
    local shm="SHMTEST_STRESS_CYCLES"
    local rounds=100
    local i
    for ((i = 0; i < rounds; i++)); do
        L_builtin shm bind -n "$shm" V
        V=(cycle_$i)
        L_unittest_eq "${V[0]}" "cycle_$i"
        L_builtin shm rm -n "$shm"
    done
    L_builtin shm rm -n "$shm" 2>/dev/null || :
}

# Regression: a variable declared `local` in a function must be shareable. Before
# the fix, `add` created a global variable that was shadowed by the local, so
# writes bypassed the shared database entirely. Now the local is bound in place;
# a forked child observes the parent's writes.
_L_test_shm_stress_local_variable() {
    local shm="SHMTEST_STRESS_LOCAL"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    local V
    L_builtin shm bind -n "$shm" V
    V=(a b c d e)
    local tmpf
    L_with_tmpfile_into tmpf
    bg() { echo "${V[*]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "a b c d e"
    L_builtin shm rm -n "$shm"
}

# Same regression check for associative `local` variables.
_L_test_shm_stress_local_variable_assoc() {
    local shm="SHMTEST_STRESS_LOCAL_ASSOC"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    local V
    L_builtin shm bind -A -n "$shm" V
    V=( [foo]=bar [baz]=qux [n1]=v1 )
    local tmpf
    L_with_tmpfile_into tmpf
    bg() { echo "${V[baz]} ${V[foo]} ${V[n1]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "qux bar v1"
    L_builtin shm rm -n "$shm"
}

###############################################################################
# `shm sync` subcommand
###############################################################################

# `sync` pushes the current bash variable contents into the shared database,
# replacing the variable's existing entry. Verified with a forked read-back.
_L_test_shm_sync_indexed() {
    local shm="SHMTEST_SYNC_IDX"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -n "$shm" V
    V=(one two three)
    L_builtin shm sync -n "$shm" V
    # A forked child reads from the shared database, not the parent's cache.
    local tmpf
    L_with_tmpfile_into tmpf
    bg() { echo "${V[2]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "three"
    L_builtin shm rm -n "$shm"
}

# `sync` for an associative array: the child reads a key that only exists in the
# shared database.
_L_test_shm_sync_assoc() {
    local shm="SHMTEST_SYNC_ASSOC"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_builtin shm bind -A -n "$shm" V
    V=( [k1]=first [k2]=second [k3]=third )
    L_builtin shm sync -A -n "$shm" V
    local tmpf
    L_with_tmpfile_into tmpf
    bg() { echo "${V[k3]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "third"
    L_builtin shm rm -n "$shm"
}

###############################################################################
# `shm ... -M NAME` (named, fixed-size anonymous mmap backend)
###############################################################################

# `add -M NAME:SIZE` binds a variable to a named fixed-size anonymous mmap; a
# forked child sees the parent's writes. The bash array is updated first, then
# serialized into the bounded region.
_L_test_shm_mmap_add_and_read() {
    local m="SHMTEST_MMAP_ADD"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" V
    V=(a b c)
    L_unittest_eq "${V[*]}" "a b c"
    L_unittest_eq "${V[1]}" "b"
    local tmpf
    tmpf="$(mktemp)"
    bg() { echo "V=${V[*]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(<"$tmpf")" "V=a b c"
    rm -f "$tmpf"
    L_builtin shm rm -M "$m"
}

# `sync -M NAME` pushes current bash contents into the named fixed-size db; a
# forked child reads the synced value from the shared mapping.
_L_test_shm_mmap_sync() {
    local m="SHMTEST_MMAP_SYNC"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" V
    V=(one two three)
    L_builtin shm sync -M "$m" V
    local tmpf
    L_with_tmpfile_into tmpf
    bg() { echo "${V[2]}" > "$1"; }
    local pid=""
    L_with_process_into pid bg "$tmpf"
    wait "$pid"
    L_unittest_eq "$(</"$tmpf")" "three"
    L_builtin shm rm -M "$m"
}

# `info -M NAME` lists the variable stored in the named mmap db.
_L_test_shm_mmap_info() {
    local m="SHMTEST_MMAP_INFO"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" V
    V=(p q r)
    local out
    out="$(L_builtin shm info -M "$m")"
    L_unittest_contains "$out" "V="
    L_builtin shm rm -M "$m"
}

# `ls -M NAME` lists only the variables bound to the selected named mmap db.
_L_test_shm_mmap_ls() {
    local m="SHMTEST_MMAP_LS"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" V
    local out
    out="$(L_builtin shm ls -M "$m")"
    L_unittest_contains "$out" "V"
    L_builtin shm rm -M "$m"
}

# `add -M NAME` (no size) to a fresh store is rejected: creation needs NAME:SIZE.
_L_test_shm_mmap_select_no_size_is_ok_only_if_created() {
    local m="SHMTEST_MMAP_CREATE"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    # Selecting a not-yet-created named store for `add` creation fails because no
    # size is given.
    L_unittest_checkexit 2 L_builtin shm bind -M "$m" V
    L_builtin shm rm -M "$m" 2>/dev/null || :
}

# `create -M NAME:SIZE` with SIZE below the header minimum is rejected.
_L_test_shm_mmap_size_too_small() {
    L_unittest_checkexit 2 L_builtin shm bind -M "small:50" V
}

# Two distinct named mmap stores are independent: writing one must not appear in
# the other, even though both are anonymous mappings in this process tree.
_L_test_shm_mmap_distinct_named_stores() {
    L_builtin shm rm -M storeA 2>/dev/null || :
    L_builtin shm rm -M storeB 2>/dev/null || :
    L_builtin shm bind -M "storeA:100000" A
    L_builtin shm bind -M "storeB:100000" B
    A=(1 2 3)
    B=(x y)
    local outa outb
    outa="$(L_builtin shm info -M storeA)"
    outb="$(L_builtin shm info -M storeB)"
    L_unittest_contains "$outa" "A="
    L_unittest_eq "$outa" "${outa#*B=}"
    L_unittest_contains "$outb" "B="
    L_unittest_eq "$outb" "${outb#*A=}"
    L_builtin shm rm -M storeA
    L_builtin shm rm -M storeB
}

# `drop` erases a single variable's data from its mmap store and unbinds it,
# without touching other variables in the same store.
_L_test_shm_mmap_drop_one_var() {
    local m="SHMTEST_MMAP_DROP"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" A
    L_builtin shm bind -M "$m:100000" B
    A=(1 2)
    B=(x y)
    L_builtin shm drop A
    # A is unbound in this shell now.
    L_unittest_eq "${A[*]}" ""
    # B is intact in the same store.
    local out
    out="$(L_builtin shm info -M "$m")"
    L_unittest_contains "$out" "B="
    L_unittest_eq "$out" "${out#*A=}"
    L_builtin shm rm -M "$m"
}

# `clear -M NAME` wipes all data from the store but keeps the backing (the named
# store can be reused). Bound variables read as empty until repopulated.
_L_test_shm_mmap_clear_keeps_backing() {
    local m="SHMTEST_MMAP_CLEAR"
    L_builtin shm rm -M "$m" 2>/dev/null || :
    L_builtin shm bind -M "$m:100000" A
    L_builtin shm bind -M "$m:100000" B
    A=(1 2)
    B=(x y)
    L_builtin shm clear -M "$m"
    local out
    out="$(L_builtin shm info -M "$m")"
    # No variables stored after clear.
    L_unittest_eq "$out" "${out#*A=}"
    L_unittest_eq "$out" "${out#*B=}"
    # The backing still exists: re-add to the same named store and write.
    L_builtin shm bind -M "$m:100000" C
    C=(hello)
    out="$(L_builtin shm info -M "$m")"
    L_unittest_contains "$out" "C="
    L_builtin shm rm -M "$m"
}

###############################################################################
# bind / local scoping
###############################################################################

# When a database already contains data for a variable (written by another
# process), binding the variable in this process must populate it from the
# database, overriding any stale local bash value.  Uses `-s` (POSIX shared
# memory) so the data survives the subshell that created it.
_L_test_shm_bind_uses_existing_db() {
    local shm="SHMTEST_BIND_EXISTING"
    L_builtin shm rm -s "$shm" 2>/dev/null || :
    L_finally L_builtin shm rm -s "$shm" 2>/dev/null
    # In a subshell, create the POSIX-shm database, bind V, and set values.
    (
        L_builtin shm bind -s "$shm" V
        V[0]=a
        V[1]=b
        V[2]=c
    )
    # In the main process, set a *different* local value before binding.
    V[0]=x
    V[1]=y
    V[2]=z
    # Bind to the *existing* database; the getter must read (a b c) from the
    # shared db, overriding the local (x y z).
    L_builtin shm bind -s "$shm" V
    L_unittest_arreq V a b c
}

# Four nested functions, each declaring `local V`.  Levels 1 and 3 bind V to
# the shared database (so their values flow through the db via the dynamic
# getter/setter); levels 2 and 4 use a plain local variable with no binding.
# Verifies that:
#  - bound variables round-trip their values through the db
#  - unbound local variables are properly isolated across scopes (writes do
#    not leak into the shared database)
#  - values set at an inner level are preserved when control returns to it
#  - an outer bound variable retains its cached value after nested calls
_L_test_shm_local_nested_bind() {
    local shm="SHMTEST_NESTED_LOCAL"
    L_builtin shm rm -n "$shm" 2>/dev/null || :
    L_finally L_builtin shm rm -n "$shm" 2>/dev/null

    _shm_nested_f4() {
        local V
        V[0]=l4
        V[1]=l4b
        L_unittest_arreq V l4 l4b
    }

    _shm_nested_f3() {
        local V
        L_builtin shm bind -n "$shm" V
        V[0]=l3
        V[1]=l3b
        L_unittest_arreq V l3 l3b
        _shm_nested_f4
        # After f4 returned (its own local V), V is still l3 from the db.
        L_unittest_arreq V l3 l3b
    }

    _shm_nested_f2() {
        local V
        V[0]=l2
        V[1]=l2b
        L_unittest_arreq V l2 l2b
        _shm_nested_f3
        # After f3 returned (which wrote l3 to the db), f2's *local* V is
        # unaffected — it remains l2 (writes did not leak into the db).
        L_unittest_arreq V l2 l2b
    }

    _shm_nested_f1() {
        local V
        L_builtin shm bind -n "$shm" V
        V[0]=l1
        V[1]=l1b
        L_unittest_arreq V l1 l1b
        _shm_nested_f2
        # After f2/f3 returned, the db holds l3 (last write by f3). Since
        # dynamic variables re-read from the db on every access, f1's V
        # reflects the latest db state — the shared database bridges scopes.
        L_unittest_arreq V l3 l3b
    }

    _shm_nested_f1
}


