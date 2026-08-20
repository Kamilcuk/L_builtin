_L_test_write_basic() {
    # Write to a pipe and read back.
    local -a p=()
    L_builtin pipe p
    local n
    L_builtin write -v n "${p[1]}" "hello"
    L_unittest_eq "$n" "5"
    L_builtin read -v data "${p[0]}" 5
    L_unittest_eq "$data" "hello"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_write_hex() {
    # Decode hex "0102" -> bytes 0x01 0x02, write, read back as hex.
    local -a p=()
    L_builtin pipe p
    L_builtin write -f hex "${p[1]}" "0102"
    L_builtin read -f hex -v data "${p[0]}" 2
    L_unittest_eq "$data" "0102"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_write_to_file() {
    # Write via L_builtin to a regular file, verify content.
    local tmpfile
    L_with_tmpfile_into tmpfile
    exec 3>"$tmpfile"
    L_builtin write -v n 3 "foobar"
    L_unittest_eq "$n" "6"
    exec 3>&-
    local content
    content="$(cat "$tmpfile")"
    L_unittest_eq "$content" "foobar"
}

_L_test_read_short() {
    # Read fewer bytes than available.
    local -a p=()
    L_builtin pipe p
    printf 'abcdefghij' >&"${p[1]}"
    L_builtin read -v data "${p[0]}" 4
    L_unittest_eq "$data" "abcd"
    L_builtin read -v rest "${p[0]}" 6
    L_unittest_eq "$rest" "efghij"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_read_nonblock_empty() {
    # Non-blocking read on an empty pipe returns immediately with empty value.
    local -a p=()
    L_builtin pipe p
    L_builtin read -n -v data "${p[0]}" 10
    L_unittest_eq "${#data}" "0"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_read_nonblock_with_data() {
    # Non-blocking read with data present returns the data.
    local -a p=()
    L_builtin pipe p
    printf 'xyz' >&"${p[1]}"
    L_builtin read -n -v data "${p[0]}" 10
    L_unittest_eq "$data" "xyz"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_readwrite_errors() {
    # Invalid FD
    L_unittest_checkexit 1 L_builtin read -v x 99999 10
    # Invalid format for read
    L_unittest_checkexit 2 L_builtin read -f bogus 0 10
    # Invalid format for write
    L_unittest_checkexit 2 L_builtin write -f bogus 0 "data"
}

_L_test_write_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin write -h
}

_L_test_write_nonblock() {
    # -n: single write call, return bytes written.
    local -a p=()
    L_builtin pipe p
    local n
    L_builtin write -n -v n "${p[1]}" "hello"
    L_unittest_eq "$n" "5"
    L_builtin read -v data "${p[0]}" 5
    L_unittest_eq "$data" "hello"
    eval "exec ${p[0]}<&-; exec ${p[1]}>&-"
}

_L_test_write_large_loops() {
    # Default: loop until all bytes written (write more than pipe buffer).
    # Use a regular file to avoid pipe-buffer blocking.
    local tmpfile
    L_with_tmpfile_into tmpfile
    exec 3>"$tmpfile"
    local n
    local big=""
    for i in $(seq 1 200000); do big+="x"; done
    L_builtin write -v n 3 "$big"
    L_unittest_eq "$n" "200000"
    exec 3>&-
    local content
    content="$(cat "$tmpfile")"
    L_unittest_eq "$content" "$big"
}

_L_test_write_help_long() {
    L_unittest_cmd -j -r "Supported formats" L_builtin write --help
}

_L_test_read_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin read -h
}
