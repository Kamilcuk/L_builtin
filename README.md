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
L_builtin eventfd counter
L_builtin memfd name
L_builtin timerfd timer
L_builtin signalfd sfd SIGUSR1
L_builtin splice -v n src dst 4096
L_builtin lseek -v pos 3 1024 CUR
L_builtin read -v data fd 4096
L_builtin write -v n fd "hello"
L_builtin fcntl fd F_GETFL
L_builtin flock path LOCK_EX
L_builtin close fd
L_builtin epoll -v ready 3:r 4:w
L_builtin poll -t 1000 -v ready 0:r 1:w
L_builtin ppoll -t 1000 -v ready -u SIGINT 0:r

# Signals
L_builtin sig block USR1 USR2
L_builtin sig unblock USR1
L_builtin sig list -v blocked
L_builtin sig run USR1 USR2 -- my_command

# Synchronization
L_builtin barrier -n b1 wait
L_builtin mutex -n m1 lock cmd
L_builtin semaphore -n s1 wait
L_builtin shm -s db add -A ASSOC key1 value1
L_builtin shm -s db info

# Variables
L_builtin replace -v out '^foo' 'bar' in
L_builtin sedvar -e 's/a/b/' var

# Utilities
L_builtin sleep 0.05
L_builtin core ls -la
L_builtin core stat file.txt
L_builtin lua 'local home = bash.get("HOME"); print(home)'
L_builtin ext readfile /etc/hostname
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
L_builtin poll -t 5000 -v ready_fds 3:r 4:w 5:p

# ready_fds contains entries like "3:r" when fd 3 is readable
```

### TCP Networking

```bash
# Server
L_builtin listen -p port_var listen_fd 127.0.0.1 0

echo "Listening on port $port_var"

L_builtin accept client_fd addr_var listen_fd

L_builtin send -v sent client_fd "Hello from server"

L_builtin shutdown client_fd WR

# Client
L_builtin connect client_fd 127.0.0.1 $port_var

L_builtin recv -v data client_fd 1024

$data"
```

### Embedded Lua

```bash
L_builtin lua '
  bash.set("MY_VAR", "hello from lua")
  local v = bash.get("MY_VAR")
  print("MY_VAR =", v)
'
```

### Core Utilities (Rust/uutils)

```bash
L_builtin core ls -la /tmp
L_builtin core stat /etc/passwd
```

### Capture Command Output

```bash
L_builtin -v output_var run echo "hello world"
echo $output_var"
```

## License

This project is licensed under the GNU General Public License v3.0 - see [LICENSE](LICENSE) for details.

## Self promotion

See my other projects: [mkdocstrings-sh](https://github.com/kamilcuk/mkdocstrings-sh), [L_lib](https://github.com/Kamilcuk/L_lib), [L_bash_profile](https://github.com/Kamilcuk/L_bash_profile).
