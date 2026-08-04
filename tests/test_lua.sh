_L_test_lua_basic() {
    local out
    out=$(L_builtin lua "print('hello from lua')")
    L_unittest_eq "$out" "hello from lua"
}

_L_test_lua_arguments() {
    local out
    out=$(L_builtin lua "print(arg[1], arg[2])" val1 val2)
    L_unittest_eq "$out" "val1	val2"
}

_L_test_lua_bind_var() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local myvar
    L_builtin lua -v myvar "return 'my_test_value'"
    L_unittest_eq "$myvar" "my_test_value"
}

_L_test_lua_help_short() {
    local out
    out=$(L_builtin lua -h)
    L_unittest_checkexit 0 L_builtin lua -h
    L_unittest_contains "$out" "Usage:"
    L_unittest_contains "$out" "--var"
}

_L_test_lua_help_long() {
    local out
    out=$(L_builtin lua --help)
    L_unittest_checkexit 0 L_builtin lua --help
    L_unittest_contains "$out" "Usage:"
    L_unittest_contains "$out" "--var"
}

_L_test_lua_missing_script() {
    L_unittest_checkexit 2 L_builtin lua 2>/dev/null
}

_L_test_lua_help_as_script_arg() {
    local out
    out=$(L_builtin lua "print(arg[1])" --help)
    L_unittest_eq "$out" '--help'
}

_L_test_lua_many_args() {
    local out
    out=$(L_builtin lua "print(#arg, arg[1], arg[5])" a b c d e)
    L_unittest_eq "$out" "5	a	e"
}

_L_test_lua_bind_var_readonly() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin lua -v rovar "return 'x'" 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}

# bash.* API tests

_L_test_lua_bash_get() {
    local myvar=hello out
    out=$(L_builtin lua "print(bash.get('myvar'))")
    L_unittest_eq "$out" "hello"
}

_L_test_lua_bash_get_unset() {
    local out
    unset -v _L_no_such_var 2>/dev/null
    out=$(L_builtin lua "print(bash.get('_L_no_such_var'))")
    L_unittest_eq "$out" "nil"
}

_L_test_lua_bash_set() {
    local newvar=
    L_builtin lua "bash.set('newvar', 'from_lua')"
    L_unittest_eq "$newvar" "from_lua"
}

_L_test_lua_bash_set_readonly() {
    local -r rovar=locked
    L_unittest_checkexit 1 L_builtin lua "bash.set('rovar', 'x')" 2>/dev/null
    L_unittest_eq "$rovar" "locked"
}

_L_test_lua_bash_get_array_removed() {
    local out
    out=$(L_builtin lua "print(bash.get_array, bash.set_array)")
    L_unittest_eq "$out" "nil	nil"
}

# --- unified bash.get ---

_L_test_lua_bash_get_indexed_array() {
    local -a arr=(one two three) out
    out=$(L_builtin lua "local t = bash.get('arr'); print(type(t), #t, t[1], t[3])")
    L_unittest_eq "$out" "table	3	one	three"
}

_L_test_lua_bash_get_array_base0() {
    local -a arr=(one two) out
    out=$(L_builtin lua "local t = bash.get('arr', 0); print(t[0], t[1])")
    L_unittest_eq "$out" "one	two"
}

_L_test_lua_bash_get_sparse_array() {
    local -a arr=([5]=x [9]=y) out
    out=$(L_builtin lua "local t = bash.get('arr', 0); print(t[5], t[9], t[0])")
    L_unittest_eq "$out" "x	y	nil"
}

_L_test_lua_bash_get_assoc() {
    local -A assoc=([k1]=v1 [k2]=v2) out
    out=$(L_builtin lua "local t = bash.get('assoc'); print(type(t), t.k1, t.k2)")
    L_unittest_eq "$out" "table	v1	v2"
}

_L_test_lua_bash_get_bad_base() {
    L_unittest_checkexit 1 L_builtin lua "bash.get('x', 2)" 2>/dev/null
}

# --- unified bash.set ---

_L_test_lua_bash_set_boolean() {
    local t= f=
    L_builtin lua "bash.set('t', true); bash.set('f', false)"
    L_unittest_eq "$t" "true"
    L_unittest_eq "$f" "false"
}

_L_test_lua_bash_set_number() {
    local i= n=
    L_builtin lua "bash.set('i', 42); bash.set('n', 3.5)"
    L_unittest_eq "$i" "42"
    L_unittest_eq "$n" "3.5"
}

_L_test_lua_bash_set_table_creates_array() {
    local -a outarr=()
    L_builtin lua "bash.set('outarr', {'x', 'y', 'z'})"
    L_unittest_eq "${#outarr[@]}" 3
    L_unittest_eq "${outarr[0]}" "x"
    L_unittest_eq "${outarr[1]}" "y"
    L_unittest_eq "${outarr[2]}" "z"
}

_L_test_lua_bash_set_table_base0() {
    local -a outarr=()
    L_builtin lua "bash.set('outarr', {[0]='a', [1]='b'}, 0)"
    L_unittest_eq "${outarr[0]}" "a"
    L_unittest_eq "${outarr[1]}" "b"
}

_L_test_lua_bash_set_table_over_scalar() {
    local sc=old
    L_builtin lua "bash.set('sc', {'n1', 'n2'})"
    L_unittest_eq "${sc[0]}" "n1"
    L_unittest_eq "${sc[1]}" "n2"
}

_L_test_lua_bash_set_table_into_assoc() {
    local -A myassoc=([old]=gone)
    L_builtin lua "bash.set('myassoc', {k1='v1', k2='v2'})"
    L_unittest_eq "${#myassoc[@]}" 2
    L_unittest_eq "${myassoc[k1]}" "v1"
    L_unittest_eq "${myassoc[k2]}" "v2"
}

_L_test_lua_bash_set_rejects_nil() {
    L_unittest_checkexit 1 L_builtin lua "bash.set('x', nil)" 2>/dev/null
}

_L_test_lua_bash_set_rejects_function() {
    L_unittest_checkexit 1 L_builtin lua "bash.set('x', print)" 2>/dev/null
}

_L_test_lua_bash_set_get_roundtrip_array() {
    local -a arr=(p q r) copy=()
    L_builtin lua "bash.set('copy', bash.get('arr'))"
    L_unittest_eq "${copy[*]}" "p q r"
}

_L_test_lua_bash_set_get_roundtrip_assoc() {
    local -A a1=([x]=1 [y]=2) a2=()
    L_builtin lua "bash.set('a2', bash.get('a1'))"
    L_unittest_eq "${a2[x]}" "1"
    L_unittest_eq "${a2[y]}" "2"
}

# --- bash.unset ---

_L_test_lua_bash_unset() {
    local gone=1 out
    out=$(L_builtin lua "print(bash.unset('gone'))")
    L_unittest_eq "$out" "true"
    L_builtin lua "bash.unset('gone')"
    L_unittest_eq "${gone+set}" ""
}

_L_test_lua_bash_unset_array() {
    local -a arr=(a b)
    L_builtin lua "bash.unset('arr')"
    L_unittest_eq "${arr+set}" ""
}

_L_test_lua_bash_unset_assoc() {
    local -A assoc=([k]=v)
    L_builtin lua "bash.unset('assoc')"
    L_unittest_eq "${assoc+set}" ""
}

_L_test_lua_bash_unset_missing() {
    local out
    out=$(L_builtin lua "print(bash.unset('_L_never_existed_xyz'))")
    L_unittest_eq "$out" "false"
}

_L_test_lua_bash_call() {
    if (( L_BASH_VERSION < 0x40300 )); then
        L_unittest_skip "bash.eval disabled on bash < 4.3+"
        return
    fi
    local out
    out=$(L_builtin lua "print(bash.eval('echo n=3 args=a b c; L_return 7'))")
    L_unittest_eq "$out" "n=3 args=a b c
7"
}

_L_test_lua_bash_call_no_args() {
    if (( L_BASH_VERSION < 0x40300 )); then
        L_unittest_skip "bash.eval disabled on bash < 4.3"
        return
    fi
    local out
    out=$(L_builtin lua "print(bash.eval('echo n=0; L_return 0'))")
    L_unittest_eq "$out" "n=0
0"
}

_L_test_lua_bash_call_missing() {
    if (( L_BASH_VERSION < 0x40300 )); then
        L_unittest_skip "bash.eval disabled on bash < 4.3"
        return
    fi
    # bash.eval now takes a command string, not a function name.
    # A non-existent command will return 127 (command not found).
    local out
    out=$(L_builtin lua "print(bash.eval('_L_no_such_command_xyz'))")
    L_unittest_eq "$out" "127"
}

_L_test_lua_bash_expand() {
    local greet=world out
    out=$(L_builtin lua "print(bash.expand('\$greet no. \$((1 + 2))'))")
    L_unittest_eq "$out" "world no. 3"
}

_L_test_lua_bash_expand_command_substitution() {
    local out
    out=$(L_builtin lua "print(bash.expand('\$(echo nested)'))")
    L_unittest_eq "$out" "nested"
}

_L_test_lua_bash_expand_list_splits_expansion() {
    # Word splitting applies to expansion results, not literal text.
    local v="a b c" out
    out=$(L_builtin lua "local t = bash.expand_list('\$v'); print(#t, t[1], t[2], t[3])")
    L_unittest_eq "$out" "3	a	b	c"
}

_L_test_lua_bash_expand_list_array() {
    local -a arr=(x y z) out
    out=$(L_builtin lua "local t = bash.expand_list('\${arr[@]}'); print(#t, t[1], t[2], t[3])")
    L_unittest_eq "$out" "3	x	y	z"
}

_L_test_lua_bash_expand_list_literal_single_word() {
    # A literal string with spaces stays a single word (no splitting).
    local out
    out=$(L_builtin lua "local t = bash.expand_list('a b c'); print(#t, t[1])")
    L_unittest_eq "$out" "1	a b c"
}

_L_test_lua_expression_arithmetic() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local out
    L_builtin lua -v out "return 6 * 7"
    L_unittest_eq "$out" "42"
}

_L_test_lua_expression_string_concat() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local out
    L_builtin lua -v out "return 'foo' .. 'bar'"
    L_unittest_eq "$out" "foobar"
}

_L_test_lua_expression_boolean() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local out
    L_builtin lua -v out "return 1 < 2"
    L_unittest_eq "$out" "true"
}

_L_test_lua_expression_bash_get_roundtrip() {
    if (( L_BASH_VERSION < 0x40300 )); then L_unittest_skip "No capture under bash <4.3 "; return; fi
    local x=10 out
    L_builtin lua -v out "return tonumber(bash.get('x')) * 2"
    L_unittest_eq "$out" "20"
}

_L_test_lua_expression_error() {
    L_unittest_checkexit 1 L_builtin lua "return nosuchfunction()" 2>/dev/null
}

_L_test_lua_syntax_error() {
    L_unittest_checkexit 1 L_builtin lua "this is not lua" 2>/dev/null
}
