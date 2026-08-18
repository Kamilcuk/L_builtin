_L_net_client_listen_accept() {
    sleep 0.1
    if ! exec 3<>/dev/tcp/127.0.0.1/"$1"; then
        echo "TCP connection failed"
        exit 1
    fi
    echo "hello from client" >&3
    exec 3>&-
}

_L_test_net_listen_accept() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd 127.0.0.1 0
    L_unittest_ne "$sfd" ""
    L_unittest_ne "$port_val" ""

    local client_pid=""
    L_with_process_into client_pid _L_net_client_listen_accept "$port_val"

    local client_fd=""
    local client_addr=""
    L_builtin accept client_fd client_addr "$sfd"

    L_unittest_ne "$client_fd" ""
    L_unittest_regex "$client_addr" "127\.0\.0\.1:[0-9]+"

    local line=""
    read -r line <&"$client_fd"
    L_unittest_eq "$line" "hello from client"

    eval "exec $client_fd<&-"
    eval "exec $sfd<&-"
}

_L_test_net_sleep_precision() {
    local start=""
    local end=""
    L_epochrealtime_usec -v start
    L_builtin sleep 0.05
    L_epochrealtime_usec -v end
    
    local elapsed=$(( end - start ))
    # 0.05 seconds = 50,000 microseconds. Let's assert it took at least 45,000 usec.
    L_unittest_success [ "$elapsed" -ge 45000 ]
}

_L_net_client_send_recv_raw() {
    L_builtin sleep 0.05
    local client_fd=""
    L_builtin connect client_fd 127.0.0.1 "$1"
    L_builtin send "$client_fd" "request_payload"
    
    local reply=""
    L_builtin recv -v reply "$client_fd" 32
    L_unittest_eq "$reply" "response_payload"

    L_builtin shutdown "$client_fd" RDWR
    eval "exec $client_fd<&-"
}

_L_test_net_connect_send_recv_raw() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd 127.0.0.1 0

    local client_pid=""
    L_with_process_into client_pid _L_net_client_send_recv_raw "$port_val"

    local accepted_fd=""
    local client_addr=""
    L_builtin accept accepted_fd client_addr "$sfd"
    
    local payload=""
    L_builtin recv -v payload "$accepted_fd" 15
    L_unittest_eq "$payload" "request_payload"

    local sent_count=""
    L_builtin send -v sent_count "$accepted_fd" "response_payload"
    L_unittest_eq "$sent_count" "16"

    eval "exec $accepted_fd<&-"
    eval "exec $sfd<&-"
}

_L_net_client_send_recv_hex() {
    L_builtin sleep 0.05
    local client_fd=""
    L_builtin connect client_fd 127.0.0.1 "$1"
    # "001122330044" represents binary bytes with multiple null-bytes!
    L_builtin send -f hex "$client_fd" "001122330044"
    eval "exec $client_fd<&-"
}

_L_test_net_connect_send_recv_hex() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd 127.0.0.1 0

    local client_pid=""
    L_with_process_into client_pid _L_net_client_send_recv_hex "$port_val"

    local accepted_fd=""
    local client_addr=""
    L_builtin accept accepted_fd client_addr "$sfd"
    
    local binary_hex=""
    L_builtin recv -f hex -v binary_hex "$accepted_fd" 6
    L_unittest_eq "$binary_hex" "001122330044"

    eval "exec $accepted_fd<&-"
    eval "exec $sfd<&-"
}

_L_test_net_nonblocking_recv() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd 127.0.0.1 0

    local client_pid=""
    L_with_process_into client_pid _L_net_client_send_recv_hex "$port_val"

    local accepted_fd=""
    local client_addr=""
    L_builtin accept accepted_fd client_addr "$sfd"

    local binary_hex=""
    L_builtin recv -f hex -v binary_hex "$accepted_fd" 6
    L_unittest_eq "$binary_hex" "001122330044"

    local empty_val="not_empty"
    L_builtin recv -n -v empty_val "$accepted_fd" 10
    L_unittest_eq "$empty_val" ""

    eval "exec $accepted_fd<&-"
    eval "exec $sfd<&-"
}

_L_test_net_defaults() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd
    L_unittest_ne "$sfd" ""
    L_unittest_ne "$port_val" ""
    L_unittest_ne "$port_val" "0"

    eval "exec $sfd<&-"
}

_L_test_net_port0_requires_p() {
    local sfd=""
    # Port is 0, so -p option is required
    L_unittest_checkexit 2 L_builtin listen sfd
}

# Test recv with poll-based timeout
# Server accepts connection but delays sending response
_L_net_client_delayed_send() {
    local port="$1"
    local delay="$2"
    local client_fd=""
    L_builtin connect client_fd 127.0.0.1 "$port"
    L_builtin sleep "$delay"
    L_builtin send "$client_fd" "delayed_response"
    L_builtin shutdown "$client_fd" RDWR
    eval "exec $client_fd<&-"
}

_L_test_net_recv_poll_timeout() {
    local sfd=""
    local port_val=""
    L_builtin listen -p port_val sfd 127.0.0.1 0

    # Start client that delays sending response by 0.5s
    local client_pid=""
    L_with_process_into client_pid _L_net_client_delayed_send "$port_val" 0.5

    local accepted_fd=""
    local client_addr=""
    L_builtin accept accepted_fd client_addr "$sfd"

    # Use poll to wait for data with 0.2s timeout (should timeout before client sends)
    local poll_result_arr=()
    L_builtin poll -t 0.2 -v poll_result_arr "$accepted_fd:r"
    # On timeout, array should be empty (0 ready fds)
    L_unittest_eq "${#poll_result_arr[@]}" "0"

    # Now wait longer with poll - should succeed when client sends
    L_builtin poll -t 1.0 -v poll_result_arr "$accepted_fd:r"
    L_unittest_ne "${#poll_result_arr[@]}" "0"

    # Receive the delayed response
    local reply=""
    L_builtin recv -v reply "$accepted_fd" 32
    L_unittest_eq "$reply" "delayed_response"

    eval "exec $accepted_fd<&-"
    eval "exec $sfd<&-"
}

# Test server that handles multiple connections sequentially
_L_net_server_multiple() {
    local count=0
    local sfd=""
    L_logrun L_builtin listen -p net_port sfd 127.0.0.1
    L_logrun L_is_integer "$net_port"
    echo "NET PORT IS $net_port"
    L_logrun L_builtin barrier wait "$net_barrier"
    while (( count < 3 )); do
        local cfd="" addr=""
        L_logrun L_builtin accept cfd addr "$sfd"
        local data=""
        L_logrun L_builtin recv -v data "$cfd" 32
        L_logrun L_builtin send "$cfd" "echo:$data"
        L_logrun L_builtin shutdown "$cfd"
        L_logrun L_builtin close "$cfd"
        (( ++count ))
    done
    eval "exec $sfd<&-"
}

_L_test_net_multiple_connections() {
    # Server function will create its own listener on the given port
    # We pass a fixed port to avoid conflict
    # local net_port net_barrier
    L_logrun L_builtin shm add net_port
    L_logrun L_builtin barrier create net_barrier 2

    local server_pid=""
    L_logrun L_with_process_into server_pid _L_net_server_multiple

    # Give server time to start listening
    L_logrun L_builtin barrier wait -t 2 "$net_barrier"
    L_logrun L_is_integer "$net_port"

    # Connect 3 clients sequentially
    for i in 1 2 3; do
        local cfd=""
        L_logrun L_builtin connect cfd 127.0.0.1 "$net_port"
        L_logrun L_builtin send "$cfd" "msg$i"
        local resp=""
        L_logrun L_builtin recv -v resp "$cfd" 32
        L_logrun L_unittest_eq "$resp" "echo:msg$i"
        L_logrun L_builtin shutdown "$cfd"
        L_logrun L_builtin close "$cfd"
    done
}
