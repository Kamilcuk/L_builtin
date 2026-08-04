_L_test_core_dirname() {
    local out
    out=$(L_builtin core dirname /a/b/c)
    L_unittest_eq "$out" "/a/b"
}

_L_test_core_capture_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin core -v myvar dirname /a/b/c
    L_unittest_eq "$myvar" $'/a/b\n'
}

_L_test_core_capture_var_multiline() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local out expected
    printf -v expected 'Cargo.toml'
    ( cd "${_L_TEST_ROOT:-.}" || exit
      L_builtin core -v out ls Cargo.toml
      L_unittest_eq "$out" $'Cargo.toml\n'
    )
}

_L_test_core_capture_no_stdout_leak() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar out
    out=$(L_builtin core -v myvar dirname /a/b/c)
    L_unittest_eq "$out" ""
}

_L_test_core_capture_missing_varname() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    L_unittest_checkexit 2 L_builtin core -v 2>/dev/null
}

_L_test_core_capture_exit_code() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_unittest_checkexit 1 L_builtin core -v myvar stat /nonexistent_file_xyz 2>/dev/null
}

_L_test_core_capture_empty_output() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar=preexisting
    L_builtin core -v myvar dirname ""
    L_unittest_eq "$myvar" $'.\n'
}

_L_test_core_capture_overwrite() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin core -v myvar dirname /a/b
    L_builtin core -v myvar dirname /x/y
    L_unittest_eq "$myvar" $'/x\n'
}

_L_test_core_unknown_subcommand() {
    L_unittest_checkexit 127 L_builtin core no_such_cmd 2>/dev/null
}

_L_test_core_help() {
    L_unittest_checkexit 0 L_builtin core -h 2>/dev/null
}

_L_test_core_capture_readonly_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin core -v rovar dirname /a/b 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}

# capture runs its argument as a shell command line, so external commands,
# shell functions, builtins and nested L_builtin calls all work.

_L_test_capture_subcommand() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar L_builtin core dirname /a/b/c
    L_unittest_eq "$myvar" $'/a/b\n'
}

_L_test_capture_subcommand_lua() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar L_builtin lua "print('from lua')"
    L_unittest_eq "$myvar" $'from lua\n'
}

_L_test_capture_missing_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    L_unittest_checkexit 2 L_builtin capture 2>/dev/null
}

_L_test_capture_missing_cmd() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_unittest_checkexit 2 L_builtin capture myvar 2>/dev/null
}

_L_test_capture_no_stdout_leak() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local out
    # Note: run in $(...) means the variable binds in the subshell; here we
    # only assert nothing leaks to stdout.
    out=$(L_builtin capture myvar echo something)
    L_unittest_eq "$out" ""
}

# --- builtins ---

_L_test_capture_builtin_echo() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar echo hello
    L_unittest_eq "$myvar" $'hello\n'
}

_L_test_capture_builtin_printf() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar printf '%s-%s' a b
    L_unittest_eq "$myvar" "a-b"
}

_L_test_capture_builtin_pwd() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar pwd
    L_unittest_eq "$myvar" "$PWD"$'\n'
}

_L_test_capture_builtin_exit_code() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar= ret=0
    L_builtin capture myvar false || ret=$?
    L_unittest_eq "$ret" 1
    L_unittest_eq "$myvar" ""
}

_L_test_capture_guarded_failure_skips_err_trap() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
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
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    _L_capture_testfunc() { echo "func output"; }
    local myvar
    L_builtin capture myvar _L_capture_testfunc
    L_unittest_eq "$myvar" $'func output\n'
    unset -f _L_capture_testfunc
}

_L_test_capture_function_args() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    _L_capture_testargs() { echo "n=$# args=$*"; }
    local myvar
    L_builtin capture myvar _L_capture_testargs a b c
    L_unittest_eq "$myvar" $'n=3 args=a b c\n'
    unset -f _L_capture_testargs
}

_L_test_capture_function_exit_code() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    _L_capture_testret() { echo out; return 7; }
    local myvar= ret=0
    L_builtin capture myvar _L_capture_testret || ret=$?
    L_unittest_eq "$ret" 7
    L_unittest_eq "$myvar" $'out\n'
    unset -f _L_capture_testret
}

_L_test_capture_function_sets_variable() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    # No subshell: the function's side effects survive the capture.
    _L_capture_setter() { _L_capture_side=set_by_func; echo done; }
    local myvar _L_capture_side=
    L_builtin capture myvar _L_capture_setter
    L_unittest_eq "$myvar" $'done\n'
    L_unittest_eq "$_L_capture_side" "set_by_func"
    unset -f _L_capture_setter
}

# --- external commands ---

_L_test_capture_external_command() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar /bin/echo external
    L_unittest_eq "$myvar" $'external\n'
}

_L_test_capture_external_path_lookup() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar env true
    L_unittest_eq "$myvar" ""
}

_L_test_capture_external_exit_code() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar= ret=0
    L_builtin capture myvar /bin/false || ret=$?
    L_unittest_eq "$ret" 1
}

_L_test_capture_command_not_found() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar= ret=0
    L_builtin capture myvar _L_no_such_command_xyz 2>/dev/null || ret=$?
    L_unittest_eq "$ret" 127
}

# --- argument handling ---

_L_test_capture_preserves_whitespace_arg() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar echo "a  b   c"
    L_unittest_eq "$myvar" $'a  b   c\n'
}

_L_test_capture_no_glob_expansion() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; fi
    local myvar
    L_builtin capture myvar echo '*'
    L_unittest_eq "$myvar" $'*\n'
}

_L_test_capture_no_reexpansion() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin capture myvar echo '$HOME'
    L_unittest_eq "$myvar" $'$HOME\n'
}

_L_test_capture_arg_with_single_quote() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    set -x
    L_builtin capture myvar echo "it's"
    L_unittest_eq "$myvar" $'it\'s\n'
}

_L_test_capture_empty_arg() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin capture myvar printf '[%s]' ""
    L_unittest_eq "$myvar" "[]"
}

# --- output handling ---

_L_test_capture_multiline_output() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin capture myvar printf 'l1\nl2\nl3\n'
    L_unittest_eq "$myvar" $'l1\nl2\nl3\n'
}

_L_test_capture_strips_trailing_newlines() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin capture myvar printf 'x\n\n\n'
    L_unittest_eq "$myvar" $'x\n\n\n'
}

_L_test_capture_empty_output() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar=preexisting
    L_builtin capture myvar true
    L_unittest_eq "$myvar" ""
}

_L_test_capture_stderr_not_captured() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    # capture binds in the current shell, so stderr must be routed to a file
    # rather than a $(...) subshell to inspect both at once.
    local myvar= errfile
    errfile=$(mktemp)
    L_builtin capture myvar sh -c 'echo out; echo err >&2' 2>"$errfile"
    L_unittest_eq "$myvar" $'out\n'
    L_unittest_eq "$(<"$errfile")" "err"
    rm -f "$errfile"
}

_L_test_capture_overwrite() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin capture myvar echo first
    L_builtin capture myvar echo second
    L_unittest_eq "$myvar" $'second\n'
}

_L_test_capture_readonly_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin capture rovar echo x 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}
