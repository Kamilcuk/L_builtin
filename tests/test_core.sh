_L_test_core_dirname() {
    local out
    out=$(L_builtin core dirname /a/b/c)
    L_unittest_eq "$out" "/a/b"
}

_L_test_core_capture_var() {
    local myvar
    L_builtin core -v myvar dirname /a/b/c
    L_unittest_eq "$myvar" "/a/b"
}

_L_test_core_capture_var_multiline() {
    local out expected
    printf -v expected 'Cargo.toml'
    ( cd "${_L_TEST_ROOT:-.}" || exit
      L_builtin core -v out ls Cargo.toml
      L_unittest_eq "$out" "$expected"
    )
}

_L_test_core_capture_no_stdout_leak() {
    local myvar out
    out=$(L_builtin core -v myvar dirname /a/b/c)
    L_unittest_eq "$out" ""
}

_L_test_core_capture_missing_varname() {
    L_unittest_checkexit 2 L_builtin core -v 2>/dev/null
}

_L_test_core_capture_exit_code() {
    local myvar
    L_unittest_checkexit 1 L_builtin core -v myvar stat /nonexistent_file_xyz 2>/dev/null
}

_L_test_core_capture_empty_output() {
    local myvar=preexisting
    L_builtin core -v myvar dirname ""
    L_unittest_eq "$myvar" "."
}

_L_test_core_capture_overwrite() {
    local myvar
    L_builtin core -v myvar dirname /a/b
    L_builtin core -v myvar dirname /x/y
    L_unittest_eq "$myvar" "/x"
}

_L_test_core_unknown_subcommand() {
    L_unittest_checkexit 127 L_builtin core no_such_cmd 2>/dev/null
}

_L_test_core_help() {
    L_unittest_checkexit 0 L_builtin core -h 2>/dev/null
}

_L_test_core_capture_readonly_var() {
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin core -v rovar dirname /a/b 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}

# capture runs its argument as a shell command line, so external commands,
# shell functions, builtins and nested L_builtin calls all work.

_L_test_capture_subcommand() {
    local myvar
    L_builtin capture myvar L_builtin core dirname /a/b/c
    L_unittest_eq "$myvar" "/a/b"
}

_L_test_capture_subcommand_lua() {
    local myvar
    L_builtin capture myvar L_builtin lua "print('from lua')"
    L_unittest_eq "$myvar" "from lua"
}

_L_test_capture_missing_var() {
    L_unittest_checkexit 2 L_builtin capture 2>/dev/null
}

_L_test_capture_missing_cmd() {
    local myvar
    L_unittest_checkexit 2 L_builtin capture myvar 2>/dev/null
}

_L_test_capture_no_stdout_leak() {
    local out
    # Note: run in $(...) means the variable binds in the subshell; here we
    # only assert nothing leaks to stdout.
    out=$(L_builtin capture myvar echo something)
    L_unittest_eq "$out" ""
}

# --- builtins ---

_L_test_capture_builtin_echo() {
    local myvar
    L_builtin capture myvar echo hello
    L_unittest_eq "$myvar" "hello"
}

_L_test_capture_builtin_printf() {
    local myvar
    L_builtin capture myvar printf '%s-%s' a b
    L_unittest_eq "$myvar" "a-b"
}

_L_test_capture_builtin_pwd() {
    local myvar
    L_builtin capture myvar pwd
    L_unittest_eq "$myvar" "$PWD"
}

_L_test_capture_builtin_exit_code() {
    local myvar= ret=0
    L_builtin capture myvar false || ret=$?
    L_unittest_eq "$ret" 1
    L_unittest_eq "$myvar" ""
}

_L_test_capture_guarded_failure_skips_err_trap() {
    # A guarded failure must behave like any ordinary command: no ERR trap,
    # no errexit exit, and the captured command's status is returned.
    local out
    out=$(
        set -e
        trap 'echo ERRTRAP' ERR
        ret=0
        L_builtin capture myvar false || ret=$?
        echo "ret=$ret"
    )
    L_unittest_eq "$out" "ret=1"
}

# --- shell functions ---

_L_test_capture_function() {
    _L_capture_testfunc() { echo "func output"; }
    local myvar
    L_builtin capture myvar _L_capture_testfunc
    L_unittest_eq "$myvar" "func output"
    unset -f _L_capture_testfunc
}

_L_test_capture_function_args() {
    _L_capture_testargs() { echo "n=$# args=$*"; }
    local myvar
    L_builtin capture myvar _L_capture_testargs a b c
    L_unittest_eq "$myvar" "n=3 args=a b c"
    unset -f _L_capture_testargs
}

_L_test_capture_function_exit_code() {
    _L_capture_testret() { echo out; return 7; }
    local myvar= ret=0
    L_builtin capture myvar _L_capture_testret || ret=$?
    L_unittest_eq "$ret" 7
    L_unittest_eq "$myvar" "out"
    unset -f _L_capture_testret
}

_L_test_capture_function_sets_variable() {
    # No subshell: the function's side effects survive the capture.
    _L_capture_setter() { _L_capture_side=set_by_func; echo done; }
    local myvar _L_capture_side=
    L_builtin capture myvar _L_capture_setter
    L_unittest_eq "$myvar" "done"
    L_unittest_eq "$_L_capture_side" "set_by_func"
    unset -f _L_capture_setter
}

# --- external commands ---

_L_test_capture_external_command() {
    local myvar
    L_builtin capture myvar /bin/echo external
    L_unittest_eq "$myvar" "external"
}

_L_test_capture_external_path_lookup() {
    local myvar
    L_builtin capture myvar env true
    L_unittest_eq "$myvar" ""
}

_L_test_capture_external_exit_code() {
    local myvar= ret=0
    L_builtin capture myvar /bin/false || ret=$?
    L_unittest_eq "$ret" 1
}

_L_test_capture_command_not_found() {
    local myvar= ret=0
    L_builtin capture myvar _L_no_such_command_xyz 2>/dev/null || ret=$?
    L_unittest_eq "$ret" 127
}

# --- argument handling ---

_L_test_capture_preserves_whitespace_arg() {
    local myvar
    L_builtin capture myvar echo "a  b   c"
    L_unittest_eq "$myvar" "a  b   c"
}

_L_test_capture_no_glob_expansion() {
    local myvar
    L_builtin capture myvar echo '*'
    L_unittest_eq "$myvar" "*"
}

_L_test_capture_no_reexpansion() {
    local myvar
    L_builtin capture myvar echo '$HOME'
    L_unittest_eq "$myvar" '$HOME'
}

_L_test_capture_arg_with_single_quote() {
    local myvar
    L_builtin capture myvar echo "it's"
    L_unittest_eq "$myvar" "it's"
}

_L_test_capture_empty_arg() {
    local myvar
    L_builtin capture myvar printf '[%s]' ""
    L_unittest_eq "$myvar" "[]"
}

# --- output handling ---

_L_test_capture_multiline_output() {
    local myvar
    L_builtin capture myvar printf 'l1\nl2\nl3\n'
    L_unittest_eq "$myvar" "l1
l2
l3"
}

_L_test_capture_strips_trailing_newlines() {
    local myvar
    L_builtin capture myvar printf 'x\n\n\n'
    L_unittest_eq "$myvar" "x"
}

_L_test_capture_empty_output() {
    local myvar=preexisting
    L_builtin capture myvar true
    L_unittest_eq "$myvar" ""
}

_L_test_capture_stderr_not_captured() {
    # capture binds in the current shell, so stderr must be routed to a file
    # rather than a $(...) subshell to inspect both at once.
    local myvar= errfile
    errfile=$(mktemp)
    L_builtin capture myvar sh -c 'echo out; echo err >&2' 2>"$errfile"
    L_unittest_eq "$myvar" "out"
    L_unittest_eq "$(<"$errfile")" "err"
    rm -f "$errfile"
}

_L_test_capture_overwrite() {
    local myvar
    L_builtin capture myvar echo first
    L_builtin capture myvar echo second
    L_unittest_eq "$myvar" "second"
}

_L_test_capture_readonly_var() {
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin capture rovar echo x 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}
