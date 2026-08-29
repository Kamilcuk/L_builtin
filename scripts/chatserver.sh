#!/usr/bin/env bash
#
# chatserver.sh - a tiny chat server/client built entirely on L_builtin fd
# primitives (listen / accept / connect / read / write + epoll or fork).
#
#   chatserver.sh unittest                 run the self-test suite
#   chatserver.sh server <ip> <port> [epoll|fork]
#                                          run the chat server (default: epoll)
#   chatserver.sh client <ip> <port>      run an interactive chat client
#
# The .so is located automatically (repo/L_builtin.so, then build/Debug or
# build/Release under the repo root); override with L_BUILTIN_SO=/path/to.so.

set -euo pipefail

SCRIPT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
SCRIPT="$SCRIPT_DIR/$(basename "${BASH_SOURCE[0]}")"
REPO=$(cd "$SCRIPT_DIR/.." && pwd)

log() { echo "chatserver: $*" >&2; }

L_lib_pull() {
  L_lib_pull_fatal() {
	  echo "$@" >&2
	  exit 123
  }
	if [[ -z "${1:-}" ]]; then
		L_lib_pull_fatal "Usage: ${FUNCNAME[0]} <destination directory>"
	fi
	mkdir -vp "$1"
	local cachef="$1"/L_lib.sh
  local version=${2:-2.0.2}
	local url="https://github.com/Kamilcuk/L_lib/releases/download/v$version/L_lib.sh"
	if [[ -z "${L_LIB_VERSION:-}" ]]; then
		# Download L_lib.sh library
		if [[ -s "$cachef" ]]; then
			echo "${FUNCNAME[0]}: Using preexisting $cachef"
		elif hash L_lib.sh 2>/dev/null; then
			. L_lib.sh -s
			echo "${FUNCNAME[0]}: Using L_lib.sh $L_LIB_VERSION from PATH"
			return
		elif hash curl 2>/dev/null; then
			echo "${FUNCNAME[0]}: Downloading L_lib.sh from $url with curl"
			curl -sSL -o "$cachef" "$url"
		elif hash wget 2>/dev/null; then
			echo "${FUNCNAME[0]}: Downloading L_lib.sh from $url with wget"
			wget -O "$cachef" "$url"
		else
			L_lib_pull_fatal "Could not find L_lib.sh and no download method available"
		fi
		if [[ -s "$cachef" ]]; then
			. "$cachef" -s
		else
			L_lib_pull_fatal "Downloading L_lib.sh from $url has failed"
		fi
	fi
}

L_lib_pull "$REPO/build" >/dev/null

load_builtin() {
  local so
  if [[ -n "${L_BUILTIN_SO:-}" ]]; then
    so="$L_BUILTIN_SO"
  else
    so="$REPO/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    so="$REPO/build/Debug/system/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    so="$REPO/build/Release/system/L_builtin.so"
  fi
  if [[ ! -f "$so" ]]; then
    echo "chatserver: error: L_builtin.so not found (run 'make build' first)." >&2
    echo "chatserver:        looked at: $so" >&2
    return 1
  fi
  enable -f "$so" L_builtin 2>/dev/null || {
    echo "chatserver: error: failed to enable L_builtin from $so" >&2
    return 1
  }
}

# ---------------------------------------------------------------------------
# Epoll-based server: one process multiplexes every client with epoll(7) and
# broadcasts each message to all connected peers (group chat). The sender's
# fd is prepended to each message so recipients know who spoke.
# ---------------------------------------------------------------------------
server_epoll() {
  local lfd=$1 EP CFD ADDR msg ready
  local -A PEERS=()
  L_builtin epoll create EP
  L_builtin epoll add "$EP" "$lfd" r
  # non-blocking accept so we can drain the pending queue without stalling.
  L_builtin fcntl setfl "$lfd" nonblock
  log "epoll server ready on fd $lfd"
  while true; do
    L_builtin epoll wait -v ready "$EP" || ready=()
    for fd in "${!ready[@]}"; do
      if [[ "$fd" -eq "$lfd" ]]; then
        while L_builtin accept CFD ADDR "$lfd" 2>/dev/null; do
          L_builtin epoll add "$EP" "$CFD" r
          PEERS[$CFD]=1
          log "accepted $ADDR on fd $CFD"
        done
        continue
      fi
      if ! L_builtin read -v msg "$fd" 4096; then
        msg=""
      fi
      if [[ -z "$msg" ]]; then
        L_builtin epoll del "$EP" "$fd"
        exec {fd}>&-
        unset PEERS[$fd]
        log "disconnected fd $fd"
        continue
      fi
      for peer in "${!PEERS[@]}"; do
        L_builtin write "$peer" "[$fd] $msg" || true
      done
    done
  done
}

# ---------------------------------------------------------------------------
# Fork-per-client server: for each accepted connection a child process is
# spawned (via L_with_process_into) that echoes the client's line back with a
# "[addr] " prefix (per-client loopback chat). SIGCHLD is ignored so children
# are auto-reaped.
# ---------------------------------------------------------------------------
server_fork() {
  local lfd=$1 CFD ADDR cpid
  trap '' SIGCHLD
  log "fork server ready on fd $lfd"
  while true; do
    if L_builtin accept -C CFD ADDR "$lfd" 2>/dev/null; then
      # -C clears close-on-exec so the forked child (a separate bash process)
      # inherits the accepted fd.
      L_with_process_into cpid _fork_child "$CFD" "$ADDR"
      exec {CFD}>&-
    fi
  done
}

# Body of one forked client handler subprocess.
_fork_child() {
  local cfd=$1 addr=$2 msg
  while L_builtin read -v msg "$cfd" 4096; do
    [[ -z "$msg" ]] && break
    L_builtin write "$cfd" "[$addr] $msg" || break
  done
  exec {cfd}>&-
}

server_main() {
  local ip=$1 port=$2 mode=${3:-epoll} LFD
  echo "chatserver: listening on $ip:$port"
  L_builtin listen LFD "$ip" "$port"
  if [[ "$mode" == fork ]]; then
    server_fork "$LFD"
  else
    server_epoll "$LFD"
  fi
}

# ---------------------------------------------------------------------------
# Interactive client: poll(2) multiplexes terminal stdin (fd 0) and the socket
# so typed lines are sent and broadcasts are printed as they arrive.
# ---------------------------------------------------------------------------
run_client() {
  local ip=$1 port=$2 CFD rdy m line
  L_builtin connect CFD "$ip" "$port"
  log "connected on fd $CFD (type messages; Ctrl-D to quit)"
  while true; do
    L_builtin poll -t 200 -v rdy 0:r "$CFD:r" || rdy=()
    for fd in "${!rdy[@]}"; do
      if [[ "$fd" -eq 0 ]]; then
        IFS= read -r line || break
        [[ -z "$line" ]] && break
        L_builtin write "$CFD" "$line" || break
      elif [[ "$fd" -eq "$CFD" ]]; then
        L_builtin read -v m "$CFD" 4096 || break
        [[ -z "$m" ]] && break
        printf '%s\n' "$m"
      fi
    done
  done
  exec {CFD}>&-
}

# ---------------------------------------------------------------------------
# Internal helpers used by the unittest (each runs in its own bash process).
# ---------------------------------------------------------------------------
_recv() {
  local ip=$1 port=$2 out=$3 CFD rdy m
  L_builtin connect CFD "$ip" "$port"
  while true; do
    L_builtin poll -t 200 -v rdy "$CFD:r" || rdy=()
    for fd in "${!rdy[@]}"; do
      if ! L_builtin read -v m "$CFD" 4096; then m=""; fi
      if [[ -z "$m" ]]; then break 2; fi
      printf '%s\n' "$m" >> "$out"
    done
  done
  exec {CFD}>&-
}

_send() {
  local ip=$1 port=$2 msg=$3 CFD
  L_builtin connect CFD "$ip" "$port"
  L_builtin write "$CFD" "$msg"
  exec {CFD}>&-
}

_send_recv() {
  local ip=$1 port=$2 msg=$3 out=$4 CFD r rdy
  L_builtin connect CFD "$ip" "$port"
  L_builtin write "$CFD" "$msg"
  L_builtin poll -t 5000 -v rdy "$CFD:r" || rdy=()
  if ((${#rdy[@]})); then
    L_builtin read -v r "$CFD" 4096 || true
  fi
  [[ -n "$r" ]] && printf '%s\n' "$r" >> "$out"
  exec {CFD}>&-
}

# ---------------------------------------------------------------------------
# Unittest: start each server implementation, exercise it with real clients,
# assert the expected bytes come back, and let L_finally tear everything down.
# ---------------------------------------------------------------------------
run_unittest() {
  local srv_pid b_pid a_pid srv2_pid a2_pid brecv forkout waited=0

  # ---- epoll broadcast: a message from one client reaches another ----
  local port=61234
  L_with_tmpfile_into brecv
  L_with_process_into srv_pid server_main 127.0.0.1 "$port" epoll
  L_with_process_into b_pid _recv 127.0.0.1 "$port" "$brecv"
  L_with_process_into a_pid _send 127.0.0.1 "$port" hello
  waited=0
  while (( waited < 100 )); do
    if grep -q hello "$brecv" 2>/dev/null; then break; fi
    sleep 0.1
    waited=$((waited + 1))
  done
  L_assert "epoll broadcast reached peer" grep -q hello "$brecv"
  L_ok "epoll broadcast OK"

  # ---- fork echo: a client's message is echoed back (prefixed) ----
  L_with_tmpfile_into forkout
  local fork_port=61235
  L_with_process_into srv2_pid server_main 127.0.0.1 "$fork_port" fork
  L_with_process_into a2_pid _send_recv 127.0.0.1 "$fork_port" ping "$forkout"
  waited=0
  while (( waited < 100 )); do
    if grep -q ping "$forkout" 2>/dev/null; then break; fi
    sleep 0.1
    waited=$((waited + 1))
  done
  L_assert "fork echo returned ping" grep -q ping "$forkout"
  L_ok "fork echo OK"

  # Kill all background processes before returning. The harness's finally
  # handler waits up to 30s for each (and the helpers ignore SIGTERM), so
  # terminate them explicitly with SIGKILL.
  kill -9 "$srv_pid" "$b_pid" "$a_pid" "$srv2_pid" "$a2_pid" 2>/dev/null
  wait "$srv_pid" "$b_pid" "$a_pid" "$srv2_pid" "$a2_pid" 2>/dev/null || true

  L_log "all unittests passed"
}

# ---------------------------------------------------------------------------
# Main dispatch via L_argparse.
# ---------------------------------------------------------------------------
L_argparse \
  prog="chatserver.sh" \
  show_default=1 \
  description="\
A tiny chat server and client built entirely on L_builtin file-descriptor \
primitives (listen / accept / connect / read / write, plus epoll(7) or a \
fork-per-client model).

  * epoll server  - one process multiplexes every client with epoll(7) and \
broadcasts each message to all connected peers (group chat, with the sender's \
fd prefixed as '[fd] ').
  * fork server   - a child bash process is spawned per connection that echoes \
each line back with a '[addr] ' prefix (per-client loopback chat).
  * client        - polls stdin (fd 0) and the socket so typed lines are sent \
and broadcasts are printed as they arrive." \
  epilog="\
Examples:
  chatserver.sh server --port 8080 epoll
  chatserver.sh client --port 8080
  chatserver.sh unittest

The L_builtin .so is located automatically (repo/L_builtin.so, then \
build/Debug or build/Release under the repo root); override with \
L_BUILTIN_SO=/path/to.so. Run 'make build' first if the builtin is missing. \
The --ip, --port and --mode options apply to the 'server' and 'client' \
subcommands (--ip defaults to 127.0.0.1, --port to 8080)." \
  -- call=subparser dest=cmd \
  { \
    name=unittest help="run the self-test suite (epoll broadcast + fork echo)" \
  } \
  { \
    name=server help="run the chat server" \
    -- --ip help="IP to bind to" default=127.0.0.1 \
    -- --port help="port to bind to" type=int default=8080 \
    -- --mode help="server implementation" choices="epoll fork" default=epoll \
  } \
  { \
    name=client help="run an interactive chat client" \
    -- --ip help="server IP to connect to" default=127.0.0.1 \
    -- --port help="server port" type=int default=8080 \
  } \
  ---- "$@"

# Enable the L_builtin builtin once, up front. Every function below (and the
# children started via L_with_process_into, which are forks of this shell,
# inheriting its enabled-builtin state) then has it available.
load_builtin

case "$cmd" in
  unittest) run_unittest ;;
  server) server_main "$ip" "$port" "$mode" ;;
  client) run_client "$ip" "$port" ;;
esac
