_L_test_core_dirname() {
    local out
    out=$(L_builtin core dirname /a/b/c)
    L_unittest_eq "$out" "/a/b"
}

_L_test_core_capture_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin -v myvar core dirname /a/b/c
    L_unittest_eq "$myvar" "/a/b"
}

_L_test_core_capture_var_multiline() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local out
    ( cd "${_L_TEST_ROOT:-.}" || exit
      L_builtin -v out core ls README.md
      L_unittest_eq "$out" "README.md"
    )
}

_L_test_core_capture_no_stdout_leak() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar out
    out=$(L_builtin -v myvar core dirname /a/b/c)
    L_unittest_eq "$out" ""
}

_L_test_core_capture_missing_varname() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    L_unittest_checkexit 2 L_builtin -v 2>/dev/null
}

_L_test_core_capture_exit_code() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_unittest_checkexit 1 L_builtin -v myvar core stat /nonexistent_file_xyz 2>/dev/null
}

_L_test_core_capture_empty_output() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar=preexisting
    L_builtin -v myvar core dirname ""
    L_unittest_eq "$myvar" "."
}

_L_test_core_capture_overwrite() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin -v myvar core dirname /a/b
    L_builtin -v myvar core dirname /x/y
    L_unittest_eq "$myvar" "/x"
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
    L_unittest_cmd ! L_builtin -v rovar core dirname /a/b 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}
