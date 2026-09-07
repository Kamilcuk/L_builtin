# L_builtin

A collection of loadable C/Rust builtins designed to extend Bash with OS-level capabilities.

These builtins are compiled into a shared library (`L_builtin.so`) which can be dynamically loaded into Bash using the `enable` command. They provide abstractions for file operations, signal masking, polling, Lua integration, networking, and core utilities via Rust/uutils.

## Table of Contents

- [L_builtin](#l_builtin)
  - [Subcommands documentation](doc/reference.md)
  - [Quick Reference](#quick-reference)
  - [Installation](#installation)
  - [Quick Start](#quick-start)
    - [Prerequisites (Build)](#prerequisites-build)
    - [Prerequisites (Run)](#prerequisites-run)
    - [Build](#build)
    - [Load into Bash](#load-into-bash)
    - [Run Tests](#run-tests)
  - [Usage Examples](#usage-examples)
    - [Sleep](#sleep)
    - [Create and Use a Pipe](#create-and-use-a-pipe)
    - [Signal Masking](#signal-masking)
    - [Poll Multiple FDs](#poll-multiple-fds)
    - [TCP Networking](#tcp-networking)
    - [Embedded Lua](#embedded-lua)
    - [Core Utilities (Rust/uutils)](#core-utilities-rustuutils)
    - [Capture Command Output](#capture-command-output)
  - [License](#license)
  - [Self promotion](#self-promotion)

## Quick Reference

```bash
# Load once per session
enable -f ./L_builtin.so L_builtin

# Help
L_builtin -h
L_builtin <subcommand> -h

# Network
L_builtin listen -p port listen_fd 0.0.0.0 0
L_builtin accept client addr listen_fd
L_builtin connect client_fd 1.2.3.4 80
L_builtin send -f hex -v n fd "deadbeef"
L_builtin recv -f hex -v data -n fd 4096
L_builtin shutdown fd WR

# File descriptors
L_builtin pipe fds
L_builtin eventfd create counter
L_builtin eventfd write $counter 3
L_builtin eventfd read $counter v
L_builtin memfd name
L_builtin timerfd create timer
L_builtin signalfd -v sfd SIGUSR1
L_builtin splice -v n src dst 4096
L_builtin lseek -v pos 3 1024 CUR
L_builtin read -v data fd 4096
L_builtin write -v n fd "hello"
L_builtin fcntl getfl fd
L_builtin fcntl setfl fd nonblock
L_builtin fcntl getfd fd
L_builtin fcntl setfd fd cloexec
L_builtin fcntl dup -v newfd fd
L_builtin fcntl list
L_builtin flock -x fd
L_builtin close fd
L_builtin epoll create efd
L_builtin poll -t 1000 -v ready 0:r 1:w
L_builtin ppoll -t 1000 -v ready -u SIGINT 0:r

# Signals
L_builtin sig block USR1 USR2
L_builtin sig unblock USR1
L_builtin sig list -v blocked
L_builtin sig run USR1 USR2 -- my_command

# Synchronization
L_builtin barrier create -n b1 b 1
L_builtin mutex create m1
L_builtin semaphore create s1 1
L_builtin shm bind VAR
L_builtin shm info
L_builtin shm ls
L_builtin shm unbind VAR
L_builtin shm drop VAR
L_builtin shm sync VAR
L_builtin shm clear
L_builtin shm rm

# Variables
L_builtin replace VAR '^foo' 'bar'
L_builtin sedvar VAR 's/a/b/'

# Utilities
L_builtin sleep 0.05
L_builtin core ls -la
L_builtin core stat file.txt
L_builtin lua 'local home = bash.get("HOME"); print(home)'
L_builtin ext basename /etc/hostname
L_builtin ext dirname /usr/local/bin/foo.sh
L_builtin ext cat /etc/hostname
L_builtin ext head -n 1 /etc/passwd
L_builtin ext id -u
L_builtin ext realpath /tmp
L_builtin ext strftime '%Y'
L_builtin ext sync
L_builtin ext whoami
L_builtin version

# Capture
L_builtin -v var run echo hello
```

## Installation

The library is one file. Download the latest release from GitHub and put in your shell's builtin path:

```bash
mkdir -vp ~/.local/lib/bash/
wget -O ~/.local/lib/bash/L_builtin.so https://github.com/Kamilcuk/L_builtin/releases/latest/download/L_builtin.so
```

Then load:

```bash
enable -f ~/.local/lib/bash/L_builtin.so L_builtin
L_builtin --help
```

> **Note:** `L_builtin.so` is a Bash loadable builtin (shared library), not a standalone executable. It must be loaded via `enable -f` inside Bash - it cannot be run directly.

## Quick Start

### Prerequisites (Build)

- Bash development headers (`/usr/include/bash/version.h` etc.)
- CMake >= 3.16
- Rust >= 1.70 (with `cargo`)
- A C compiler (e.g., clang or gcc)

### Prerequisites (Run)

- Bash (any version with `enable -f` support)
- `L_builtin.so` (built artifact)

### Build

```bash
# Build bash 5.2
make bash-build BASH=5.2

# Build code for bash 5.2
make build BASH=5.2 BASHES=5.2

# Build single muilti-version dispatcher.
make dispatcher-build BASHES="5.3 5.2"
```

Creates `./L_builtin.so`.

### Load into Bash

```bash
# Interactive session with builtin loaded
make sh

# Or manually:
enable -f ./L_builtin.so L_builtin
```

### Run Tests

```bash
make test
# or
./runtests.sh ./L_builtin.so --help
```

This compiles the module, runs all modular test files in `tests/`, and executes style checks, formatting validation, and static analysis.

## Usage Examples

Narrative walkthroughs of common tasks. For the complete per-subcommand reference (every flag, exit code, edge case), see [doc/reference.md](doc/reference.md).

### Sleep

```bash
L_builtin sleep 0.001  # 1 millisecond
```

### Create and Use a Pipe

```bash
L_builtin pipe mypipe

echo "hello" >&${mypipe[1]}

read -r line <&${mypipe[0]}

$line"
```

### Signal Masking

```bash
# Block SIGUSR1/SIGUSR2 in the current shell
L_builtin sig block USR1 USR2

# List currently blocked signals
L_builtin sig list

# Or capture them into an indexed array
L_builtin sig list -v blocked
echo "${blocked[@]}"

# Unblock signals (persistent)
L_builtin sig unblock USR1

# Run a command with signals temporarily unblocked (caller's mask restored after)
L_builtin sig run USR1 USR2 -- my_command
```

### Poll Multiple FDs

```bash
exec 3<> /tmp/L_poll_example
echo data >&3
L_builtin poll -t 5000 -v ready_fds 3:r 4:w

# ready_fds is a sparse indexed array keyed by fd:
#   ${ready_fds[3]} = r    # fd 3 was readable
#   ${ready_fds[4]} = n    # fd 4 was invalid/hangup
exec 3<&-
```

### TCP Networking

`listen`, `accept`, `connect`, `send`, `recv`, `shutdown` work on real BSD
sockets. A minimal round-trip — the server runs in a backgrounded subshell
and publishes its bound port back to the parent through an `L_builtin pipe`:

```bash
enable -f ./L_builtin.so L_builtin

# Open a pipe for the server → parent port handoff.
L_builtin pipe pp

(
  L_builtin listen -p port_var listen_fd 127.0.0.1 0
  echo "$port_var" >&${pp[1]}              # publish port to the parent
  L_builtin accept client_fd addr_var $listen_fd
  L_builtin send -v sent $client_fd "Hello from server"
  L_builtin close $client_fd
  L_builtin close $listen_fd
) &
server_pid=$!

# Parent reads the port from the pipe (no race, no polling).
IFS= read -r port <&${pp[0]}

L_builtin connect client_fd 127.0.0.1 $port
L_builtin recv -v data $client_fd 1024
L_builtin close $client_fd
echo "received: $data"   # received: Hello from server

wait $server_pid
L_builtin close ${pp[0]} ${pp[1]}
```

Variables set in the server subshell are not visible to the parent (they live
in the subshell's variable table). The pipe above is the channel; other
options are the filesystem, or any of the sync primitives (`shm`, `barrier`,
...).

### Embedded Lua

Lua shares the bash process heap: variables are live in both directions, no
serialization. Three of the interop primitives:

```bash
# Scalar round-trip — read HOME into lua, transform, write back.
L_builtin lua '
  local home = bash.get("HOME")
  bash.set("HOME_PARENT", home:gsub("/[^/]*$", ""))
'

# Indexed array round-trip — bash → lua → bash.
DIRS=(/usr/local/bin /opt/bin /home/kamil/bin)
L_builtin lua '
  for i, dir in ipairs(bash.get("DIRS")) do
    bash.set("PATH", dir .. ":" .. bash.get("PATH"))
  end
'

# Associative array round-trip, plus a function call.
declare -A config=([host]=db1 [port]=5432)
L_builtin lua '
  local c = bash.get("config")
  c.host = c.host .. ".internal"   -- mutate in-place
  c.replicas = 3                    -- add a key
  bash.set("config", c)
'
# config is now ( [host]=db1.internal [port]=5432 [replicas]=3 )

# bash.eval runs a bash command string. It returns the exit status (integer);
# the command's stdout goes straight to the calling bash's stdout, not back
# into lua. So to capture command output you read it from lua's pcall return
# or use bash.get on a variable the command assigned to.
L_builtin -v out lua 'bash.eval("FOO=hello; export FOO"); print(bash.get("FOO"))'
echo "$out"   # hello
```

The full lua API (`bash.get/set/unset/eval/expand/expand_list`) is documented
under [lua](doc/reference.md#lua).

### Core Utilities (Rust/uutils)

```bash
L_builtin core ls -la /tmp
L_builtin core stat /etc/passwd
```

### Capture Command Output

```bash
L_builtin -v output_var run echo "hello world"
echo "$output_var"   # hello world
```

## License

This project is licensed under the GNU General Public License v3.0 - see [LICENSE](LICENSE) for details.

## Self promotion

See my other projects: [mkdocstrings-sh](https://github.com/kamilcuk/mkdocstrings-sh), [L_lib](https://github.com/Kamilcuk/L_lib), [L_bash_profile](https://github.com/Kamilcuk/L_bash_profile).
