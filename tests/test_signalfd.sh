# Tests for the `L_builtin signalfd` subcommand: signalfd(2) signal fds.

_L_test_signalfd_create() {
    L_builtin signalfd -v SF SIGUSR1
    L_unittest_cmd -jr '^[0-9]+$' echo "$SF"
    L_builtin close "$SF"
}

_L_test_signalfd_all_signals() {
    # No signals listed => covers every signal.
    L_builtin signalfd -v SF
    L_unittest_cmd -jr '^[0-9]+$' echo "$SF"
    L_builtin close "$SF"
}

_L_test_signalfd_block_delivery() {
    # -b blocks SIGUSR1 so it is delivered through the fd, not the default action.
    L_builtin signalfd -b -v SF SIGUSR1
    kill -USR1 $BASHPID 2>/dev/null
    local got
    L_builtin read -f hex -v got "$SF" 128
    # ssi_signo for SIGUSR1 (10) is the first little-endian word.
    L_unittest_cmd -jr '^0a00' echo "$got"
    L_builtin close "$SF"
}

_L_test_signalfd_nonblock_empty() {
    L_builtin signalfd -n -v SF SIGUSR2
    local got
    L_builtin read -n -v got "$SF" 128
    L_unittest_eq "${#got}" "0"
    L_builtin close "$SF"
}

_L_test_signalfd_unknown_signal() {
    L_unittest_checkexit 2 L_builtin signalfd -v X BOGUS
}

_L_test_signalfd_help_short() {
    L_unittest_cmd -j -r "usage" L_builtin signalfd -h
}

_L_test_signalfd_help_long() {
    L_unittest_cmd -j -r "signalfd\(2\)" L_builtin signalfd --help
}
