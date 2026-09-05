# L_builtin

A collection of loadable C/Rust builtins designed to extend Bash with OS-level capabilities.

These builtins are compiled into a shared library (`L_builtin.so`) which can be dynamically loaded into Bash using the `enable` command. They provide abstractions for file operations, signal masking, polling, Lua integration, networking, and core utilities via Rust/uutils.

## Table of Contents

- [L_builtin](#l_builtin)
  - [Quick Reference](#quick-reference)
  - [Installation](#installation)
  - [Quick Start](#quick-start)
    - [Prerequisites (Build)](#prerequisites-build)
    - [Prerequisites (Run)](#prerequisites-run)
    - [Build](#build)
    - [Load into Bash](#load-into-bash)
    - [Run Tests](#run-tests)
  - [Features](#features)
  - [Usage Examples](#usage-examples)
    - [Sleep](#sleep)
    - [Create and Use a Pipe](#create-and-use-a-pipe)
    - [Signal Masking](#signal-masking)
    - [Poll Multiple FDs](#poll-multiple-fds)
    - [TCP Networking](#tcp-networking)
    - [Embedded Lua](#embedded-lua)
    - [Core Utilities (Rust/uutils)](#core-utilities-rustuutils)
    - [Capture Command Output](#capture-command-output)
  - [Subcommand Reference](doc/commands.md)
  - [License](#license)
  - [Self promotion](#self-promotion)

## Quick Reference

```bash
# Load once per session
enable -f ./build/L_builtin.so L_builtin

# Help
L_builtin -h
L_builtin <subcommand> -h

# File/Process
L_builtin lseek -v pos 3 1024 CUR
L_builtin pipe fds
L_builtin sleep 0.05

# Signals
L_builtin sigmask -s SIGUSR1
L_builtin sigunmask -s SIGUSR1 cmd

# Polling
L_builtin poll -t 1000 -v ready 0:r 1:w
L_builtin ppoll -t 1000 -v ready -u SIGINT 0:r

# Networking
L_builtin listen -p port listen_fd 0.0.0.0 0
L_builtin accept client addr listen_fd
L_builtin connect client_fd 1.2.3.4 80
L_builtin send -f hex -v n fd "deadbeef"
L_builtin recv -f hex -v data -n fd 4096
L_builtin shutdown fd WR

# Lua
L_builtin lua 'local home = bash.get("HOME"); print(home)'

# Core utils
L_builtin core ls -la
L_builtin core stat file.txt

# Capture
L_builtin capture var echo hello
```

## Installation

The library is one file. Download the latest release from GitHub and put in your shell's builtin path:

```bash
mkdir -vp ~/.local/lib/bash/
wget -O ~/.local/lib/bash/L_builtin.so https://github.com/Kamilcuk/L_builtin/releases/latest/download/L_builtin-linux-x86_64-bash-5.3.so
```

Then load in your `.bashrc` or interactively:

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
make build

# or manually:
cmake -S . -B build -DL_DEV=1
cmake --build build
```

Creates `build/L_builtin.so`.

### Load into Bash

```bash
# Interactive session with builtin loaded
make sh

# Or manually:
enable -f ./build/L_builtin.so L_builtin
```

### Run Tests

```bash
make test
```

This compiles the module, runs all modular test files in `tests/`, and executes style checks, formatting validation, and static analysis.

## Features

- **`lseek`**: Reposition read/write file offset with `SEEK_SET`/`SEEK_CUR`/`SEEK_END`
  ```bash
  L_builtin lseek -v pos 3 1024 CUR
  ```
- **`pipe`**: Create a uni-directional data channel (stores FDs in array)
  ```bash
  L_builtin pipe fds
  ```
- **`sleep`**: Sub-second sleep (microsecond resolution)
  ```bash
  L_builtin sleep 0.05
  ```
- **`sigmask`**: Block or unblock signal delivery; print current mask
  ```bash
  L_builtin sigmask -s SIGUSR1
  ```
- **`sigunmask`**: Temporarily unblock signals and execute a command
  ```bash
  L_builtin sigunmask -s SIGUSR1 my_command
  ```
- **`poll`**: Wait for multiple file descriptors to become ready for I/O
  ```bash
  L_builtin poll -t 1000 -v ready 0:r 1:w
  ```
- **`ppoll`**: Wait for multiple file descriptors to become ready for I/O with signal unblocking
  ```bash
  L_builtin ppoll -t 1000 -v ready -u SIGINT 0:r
  ```
- **`listen`**: Create a listening TCP socket (ephemeral port support)
  ```bash
  L_builtin listen -p port listen_fd 0.0.0.0 0
  ```
- **`accept`**: Accept a new connection on a listening socket
  ```bash
  L_builtin client addr listen_fd
  ```
- **`connect`**: Establish an outgoing TCP connection
  ```bash
  L_builtin connect client_fd 1.2.3.4 80
  ```
- **`shutdown`**: Semi-close a full-duplex TCP socket (`RD`, `WR`, `RDWR`)
  ```bash
  L_builtin shutdown fd WR
  ```
- **`send`**: Transmit raw or hex-encoded data over a socket
  ```bash
  L_builtin send -f hex -v n fd "deadbeef"
  ```
- **`recv`**: Receive up to N bytes (raw or hex-encoded; non-blocking option)
  ```bash
  L_builtin recv -f hex -v data -n fd 4096
  ```
- **`core ls`**: `ls` implementation
  ```bash
  L_builtin core ls -la
  ```
- **`core stat`**: File status display
  ```bash
  L_builtin core stat file.txt
  ```
- **`capture`**: Run a command with stdout captured into a variable
  ```bash
  L_builtin capture var echo hello
  ```
- **`lua`**: Execute inline LuaJIT code within the Bash process; exposes a `bash` table for shell interaction
  ```bash
  L_builtin lua 'local home = bash.get("HOME"); print(home)'
  ```

## Usage Examples

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
# Block SIGUSR1
L_builtin sigmask -s SIGUSR1

# Run command with SIGUSR1 unblocked
L_builtin sigunmask -s SIGUSR1 my_command
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
L_builtin capture output_var echo "hello world"

$output_var"
```

The full reference for every subcommand lives in [doc/commands.md](doc/commands.md).

## License

This project is licensed under the GNU General Public License v3.0 - see [LICENSE](LICENSE) for details.

## Self promotion

[mkdocstrings-sh](https://github.com/kamilcuk/mkdocstrings-sh), [L_lib](https://github.com/Kamilcuk/L_lib), [L_bash_profile](https://github.com/Kamilcuk/L_bash_profile).