# L_builtin



A collection of loadable C/Rust builtins designed to extend Bash with OS-level capabilities.



These builtins are compiled into a shared library (`L_builtin.so`) which can be dynamically loaded into Bash using the `enable` command. They provide abstractions for file operations, signal masking, polling, Lua integration, networking, and core utilities via Rust/uutils.



## Table of Contents

<!-- TOC_GEN_START -->
- [L_builtin](#l_builtin)
  - [Table of Contents](#table-of-contents)
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
  - [Subcommand Reference](#subcommand-reference)
    - [L_builtin accept](#l_builtin-accept)
    - [L_builtin capture](#l_builtin-capture)
    - [L_builtin connect](#l_builtin-connect)
    - [L_builtin core](#l_builtin-core)
    - [L_builtin eventfd](#l_builtin-eventfd)
      - [L_builtin eventfd create](#l_builtin-eventfd-create)
      - [L_builtin eventfd write](#l_builtin-eventfd-write)
      - [L_builtin eventfd read](#l_builtin-eventfd-read)
    - [L_builtin epoll](#l_builtin-epoll)
      - [L_builtin epoll create](#l_builtin-epoll-create)
      - [L_builtin epoll add](#l_builtin-epoll-add)
      - [L_builtin epoll mod](#l_builtin-epoll-mod)
      - [L_builtin epoll del](#l_builtin-epoll-del)
      - [L_builtin epoll wait](#l_builtin-epoll-wait)
      - [L_builtin epoll close](#l_builtin-epoll-close)
    - [L_builtin ext](#l_builtin-ext)
      - [L_builtin ext asort](#l_builtin-ext-asort)
      - [L_builtin ext basename](#l_builtin-ext-basename)
      - [L_builtin ext cat](#l_builtin-ext-cat)
      - [L_builtin ext chmod](#l_builtin-ext-chmod)
      - [L_builtin ext csv](#l_builtin-ext-csv)
      - [L_builtin ext cut](#l_builtin-ext-cut)
      - [L_builtin ext dirname](#l_builtin-ext-dirname)
      - [L_builtin ext dsv](#l_builtin-ext-dsv)
      - [L_builtin ext enable_mypid](#l_builtin-ext-enable_mypid)
      - [L_builtin ext false](#l_builtin-ext-false)
      - [L_builtin ext fdflags](#l_builtin-ext-fdflags)
      - [L_builtin ext finfo](#l_builtin-ext-finfo)
      - [L_builtin ext fltexpr](#l_builtin-ext-fltexpr)
      - [L_builtin ext getconf](#l_builtin-ext-getconf)
      - [L_builtin ext head](#l_builtin-ext-head)
      - [L_builtin ext hello](#l_builtin-ext-hello)
      - [L_builtin ext id](#l_builtin-ext-id)
      - [L_builtin ext kv](#l_builtin-ext-kv)
      - [L_builtin ext lcut](#l_builtin-ext-lcut)
      - [L_builtin ext ln](#l_builtin-ext-ln)
      - [L_builtin ext logname](#l_builtin-ext-logname)
      - [L_builtin ext mkdir](#l_builtin-ext-mkdir)
      - [L_builtin ext mkfifo](#l_builtin-ext-mkfifo)
      - [L_builtin ext mktemp](#l_builtin-ext-mktemp)
      - [L_builtin ext echo](#l_builtin-ext-echo)
      - [L_builtin ext pathchk](#l_builtin-ext-pathchk)
      - [L_builtin ext print](#l_builtin-ext-print)
      - [L_builtin ext printenv](#l_builtin-ext-printenv)
      - [L_builtin ext push](#l_builtin-ext-push)
      - [L_builtin ext realpath](#l_builtin-ext-realpath)
      - [L_builtin ext rm](#l_builtin-ext-rm)
      - [L_builtin ext rmdir](#l_builtin-ext-rmdir)
      - [L_builtin ext seq](#l_builtin-ext-seq)
      - [L_builtin ext setpgid](#l_builtin-ext-setpgid)
      - [L_builtin ext sleep](#l_builtin-ext-sleep)
      - [L_builtin ext stat](#l_builtin-ext-stat)
      - [L_builtin ext strftime](#l_builtin-ext-strftime)
      - [L_builtin ext strptime](#l_builtin-ext-strptime)
      - [L_builtin ext sync](#l_builtin-ext-sync)
      - [L_builtin ext tee](#l_builtin-ext-tee)
      - [L_builtin ext template](#l_builtin-ext-template)
      - [L_builtin ext true](#l_builtin-ext-true)
      - [L_builtin ext tty](#l_builtin-ext-tty)
      - [L_builtin ext uname](#l_builtin-ext-uname)
      - [L_builtin ext unlink](#l_builtin-ext-unlink)
      - [L_builtin ext whoami](#l_builtin-ext-whoami)
    - [L_builtin fcntl](#l_builtin-fcntl)
      - [L_builtin fcntl getfl](#l_builtin-fcntl-getfl)
      - [L_builtin fcntl setfl](#l_builtin-fcntl-setfl)
      - [L_builtin fcntl getfd](#l_builtin-fcntl-getfd)
      - [L_builtin fcntl setfd](#l_builtin-fcntl-setfd)
      - [L_builtin fcntl dup](#l_builtin-fcntl-dup)
    - [L_builtin listen](#l_builtin-listen)
    - [L_builtin lua](#l_builtin-lua)
    - [L_builtin memfd](#l_builtin-memfd)
    - [L_builtin mutex](#l_builtin-mutex)
      - [L_builtin mutex create](#l_builtin-mutex-create)
      - [L_builtin mutex open](#l_builtin-mutex-open)
      - [L_builtin mutex lock](#l_builtin-mutex-lock)
      - [L_builtin mutex unlock](#l_builtin-mutex-unlock)
      - [L_builtin mutex close](#l_builtin-mutex-close)
      - [L_builtin mutex destroy](#l_builtin-mutex-destroy)
    - [L_builtin pipe](#l_builtin-pipe)
    - [L_builtin poll](#l_builtin-poll)
    - [L_builtin ppoll](#l_builtin-ppoll)
    - [L_builtin recv](#l_builtin-recv)
    - [L_builtin semaphore](#l_builtin-semaphore)
      - [L_builtin semaphore create](#l_builtin-semaphore-create)
      - [L_builtin semaphore open](#l_builtin-semaphore-open)
      - [L_builtin semaphore wait](#l_builtin-semaphore-wait)
      - [L_builtin semaphore post](#l_builtin-semaphore-post)
      - [L_builtin semaphore close](#l_builtin-semaphore-close)
      - [L_builtin semaphore destroy](#l_builtin-semaphore-destroy)
    - [L_builtin send](#l_builtin-send)
    - [L_builtin shutdown](#l_builtin-shutdown)
    - [L_builtin sigmask](#l_builtin-sigmask)
    - [L_builtin sigunmask](#l_builtin-sigunmask)
    - [L_builtin sleep](#l_builtin-sleep)
    - [L_builtin shm](#l_builtin-shm)
      - [L_builtin shm add](#l_builtin-shm-add)
      - [L_builtin shm rm](#l_builtin-shm-rm)
      - [L_builtin shm unbind](#l_builtin-shm-unbind)
      - [L_builtin shm info](#l_builtin-shm-info)
      - [L_builtin shm ls](#l_builtin-shm-ls)
      - [L_builtin shm sync](#l_builtin-shm-sync)
    - [L_builtin splice](#l_builtin-splice)
    - [L_builtin signalfd](#l_builtin-signalfd)
    - [L_builtin timerfd](#l_builtin-timerfd)
      - [L_builtin timerfd create](#l_builtin-timerfd-create)
    - [L_builtin lseek](#l_builtin-lseek)
    - [L_builtin barrier](#l_builtin-barrier)
      - [L_builtin barrier create](#l_builtin-barrier-create)
      - [L_builtin barrier open](#l_builtin-barrier-open)
      - [L_builtin barrier wait](#l_builtin-barrier-wait)
      - [L_builtin barrier close](#l_builtin-barrier-close)
      - [L_builtin barrier reset](#l_builtin-barrier-reset)
      - [L_builtin barrier destroy](#l_builtin-barrier-destroy)
  - [License](#license)
  - [Self promotion](#self-promotion)
<!-- TOC_GEN_END -->


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


## Subcommand Reference

<!-- README_GEN_START -->

This section is **auto-generated** from `L_builtin <subcommand> -h`.
Regenerate with `make readme` or `uv run --with markdown-it-py scripts/gen_readme.py`. Do not edit by hand between the markers.

### `L_builtin accept`

```
L_builtin accept: usage: CLIENTFD_VAR ADDR_VAR LISTENFD

Accept an incoming connection on the listening socket file descriptor LISTENFD.
The new socket file descriptor for the client is stored in CLIENTFD_VAR.
The client's address (IP:PORT) is stored in ADDR_VAR.

Exit Status:
Returns success unless accept fails or variable binding fails.
```

### `L_builtin capture`

```
L_builtin capture: usage: VAR <command> [args...]

Run <command> with its stdout captured into the shell variable VAR
(trailing newlines stripped, like $(...)). The command runs through the
shell, so external commands, functions, builtins and L_builtin subcommands
all work uniformly.
```

### `L_builtin connect`

```
L_builtin connect: usage: CLIENTFD_VAR IP PORT

Establish an outgoing connection to IP on PORT, and store the resulting
socket file descriptor in CLIENTFD_VAR.

Exit Status:
Returns success unless connection fails or variable binding fails.
```

### `L_builtin core`

```
L_builtin core: usage: <subcommand> [options] [args]

Core utilities via uutils/coreutils

Available subcommands:
    ls       List directory contents
    stat     Display file status
    dirname  Strip last component from file name
    rm       Remove files or directories
    tee      Copy stdin to each FILE and stdout
    sleep    Delay for a specified amount of time

Use 'L_builtin core <subcommand> --help' for more information.
```

### `L_builtin eventfd`

```
L_builtin eventfd: usage: create [-n] [-s] [-C] VAR [INITVAL] | write FD [VALUE] | read FD [VAR]

Create an eventfd(2) counting file descriptor and read/write its 64-bit counter.

Subcommands:
  create [-n] [-s] [-C] VAR [INITVAL]
                        Create an eventfd(2) and store its file descriptor in the
                        shell variable VAR. EFD_CLOEXEC is set by default; -C
                        clears it. INITVAL initializes the counter (default 0).
  write FD [VALUE]      Write VALUE (a 64-bit unsigned integer, default 1) into
                        the eventfd FD, adding it to the counter. VALUE is carried
                        as 8 bytes in native byte order.
  read FD [VAR]         Read the eventfd FD counter (an 8-byte native-endian
                         u64), resetting it to 0. If the counter was 0, a blocking
                         fd blocks; create the fd with -n (EFD_NONBLOCK) for
                         non-blocking operation (read then fails with EAGAIN).
                         Without EFD_SEMAPHORE read returns the full counter; with
                         it read returns 1 and decrements by 1. If VAR is given the
                          counter value is stored there, otherwise it is printed.


The file descriptor is a real OS descriptor (as with the `close`, `lseek`,
`timerfd` and `signalfd` subcommands), so it can be polled through the `poll`/
`ppoll` subcommands and closed with `close`.

Exit Status:
  Returns success unless eventfd(2) fails or the variable cannot be bound.

Examples:
  L_builtin eventfd create -n ev          # counter=0, non-blocking, fd in $ev
  L_builtin eventfd write "$ev" 5        # counter += 5
  L_builtin eventfd read "$ev" val       # val=5, counter reset to 0
  L_builtin eventfd write "$ev" 1        # counter += 1
  L_builtin eventfd read "$ev"           # prints 1
  L_builtin close "$ev"
```

#### `L_builtin eventfd create`

```
L_builtin eventfd create: usage: create [-n] [-s] [-C] VAR [INITVAL]

Create an eventfd(2) and store its file descriptor in the shell variable VAR.

Options:
  -n   EFD_NONBLOCK (reads/writes do not block).
  -s   EFD_SEMAPHORE: read returns 1 instead of the counter value.
  -C   Do not set EFD_CLOEXEC (it is set by default).

INITVAL initializes the 64-bit counter (default 0).

Examples:
  L_builtin eventfd create ev
  L_builtin eventfd create -n ev 5
  L_builtin eventfd create -s -n ev
  L_builtin eventfd create -C ev 100
```

#### `L_builtin eventfd write`

```
L_builtin eventfd write: usage: write FD [VALUE]

Write VALUE into the eventfd FD, adding it to the 64-bit counter.

VALUE is a 64-bit unsigned integer carried as 8 bytes in native byte order
(default 1). A successful write adds VALUE to the counter. Writing a non-zero
value that would overflow the counter blocks (or, for a non-blocking fd, fails
with EAGAIN); writing the value 2**64-1 (0xFFFFFFFFFFFFFFFF) when the counter is
non-zero fails with EINVAL.

Examples:
  L_builtin eventfd write "$ev"
  L_builtin eventfd write "$ev" 42
```

#### `L_builtin eventfd read`

```
L_builtin eventfd read: usage: read FD [VAR]

Read the 64-bit counter from the eventfd FD, resetting it to 0.

Without EFD_SEMAPHORE, read returns the whole counter value and resets it to 0.
With EFD_SEMAPHORE (created via 'create -s'), read returns 1 and decrements the
counter by 1. If the counter is 0, a blocking fd blocks until it becomes
non-zero; create the fd with -n (EFD_NONBLOCK) for non-blocking operation (read
then fails with EAGAIN).

If VAR is given, the counter value is stored in the shell variable VAR as an
integer. Otherwise it is printed to stdout.

Examples:
  L_builtin eventfd read "$ev" val
  L_builtin eventfd read "$ev"
```

### `L_builtin epoll`

```
L_builtin epoll: usage: create FD_VAR | add EPOLLFD FD [r|w|p|t] | mod EPOLLFD FD [r|w|p|t] | del EPOLLFD FD | wait [-t SECS] [-v ARR] EPOLLFD | close EPOLLFD

Scalable I/O event notification via epoll(7), the Linux-specific readiness
mechanism that scales O(1) per ready fd (unlike poll/ppoll, which scan the full
fd set). The fds it produces compose with the rest of the fd subcommands
(send/recv/accept/close, timerfd, eventfd, signalfd).

Subcommands:
  create FD_VAR             Create an epoll instance and store its fd in FD_VAR.
                              The fd is close-on-exec.
  add EPOLLFD FD [events]   Register FD on EPOLLFD (EPOLL_CTL_ADD). EVENTS defaults
                             to r; see EVENTS tokens below.
  mod EPOLLFD FD [events]   Change FD's event mask on EPOLLFD (EPOLL_CTL_MOD).
  del EPOLLFD FD            Stop watching FD on EPOLLFD (EPOLL_CTL_DEL).
  wait [-t SECS] [-v ARR] EPOLLFD
                            Block until fds on EPOLLFD are ready. With -v ARR, every
                             ready fd is stored as a sparse array entry ARR[FD]=
                             events. -t SECS sets a timeout (durations like
                             '1.5', '500ms' accepted); without -t it blocks
                             forever.
  close EPOLLFD             Close the epoll instance fd (close(2)).

EVENTS tokens (add/mod):  r EPOLLIN | w EPOLLOUT | p EPOLLPRI | t EPOLLET
                          (edge-triggered; combine, e.g. 'rw', 'rt').
Readiness tokens (wait -> ARR[FD]): r w p | h EPOLLHUP | e EPOLLERR | t EPOLLET

The fd is just an integer bash variable; there is no handle registry.

Examples:
   # Wait for two pipes to become readable (edge-triggered)
   L_builtin epoll create -n ep
   L_builtin epoll add $ep ${p[0]} rt
   L_builtin epoll add $ep ${p[1]} rt
   L_builtin epoll wait -v ready $ep
   for fd in "${!ready[@]}"; do echo "fd $fd: ${ready[$fd]}"; done
   L_builtin epoll close $ep
```

#### `L_builtin epoll create`

```
L_builtin epoll create: usage: create FD_VAR

Create an epoll instance (epoll_create1(2)) and store its file descriptor in
the shell variable FD_VAR. The fd is close-on-exec (EPOLL_CLOEXEC). The fd
becomes readable (POLLIN) when any watched fd is ready, so it can be polled
together with other fds (see poll/ppoll).

Examples:
   L_builtin epoll create ep
```

#### `L_builtin epoll add`

```
L_builtin epoll add: usage: add EPOLLFD FD [events]

Register FD on the epoll instance EPOLLFD via epoll_ctl(2) EPOLL_CTL_ADD.

EVENTS defaults to 'r' (EPOLLIN). Tokens: r/EPOLLIN, w/EPOLLOUT, p/EPOLLPRI,
t/EPOLLET (edge-triggered). Combine them, e.g. 'rw' or 'rt'.

Examples:
   L_builtin epoll add $ep 3          # watch fd 3 for reads (default)
   L_builtin epoll add $ep 3 w        # watch fd 3 for writes
   L_builtin epoll add $ep 3 rt       # edge-triggered read on fd 3
```

#### `L_builtin epoll mod`

```
L_builtin epoll mod: usage: mod EPOLLFD FD [events]

Change the event mask of FD on EPOLLFD via epoll_ctl(2) EPOLL_CTL_MOD.

EVENTS defaults to 'r'. See `add` for the token meaning.

Examples:
   L_builtin epoll mod $ep 3 rw       # now also watch fd 3 for writes
```

#### `L_builtin epoll del`

```
L_builtin epoll del: usage: del EPOLLFD FD

Stop watching FD on EPOLLFD via epoll_ctl(2) EPOLL_CTL_DEL. Takes no event
argument; a trailing token is rejected as 'too many arguments'.

Examples:
   L_builtin epoll del $ep 3
```

#### `L_builtin epoll wait`

```
L_builtin epoll wait: usage: wait [-t SECS] [-v ARR] EPOLLFD

Block until one or more fds registered on EPOLLFD are ready (epoll_wait(2)).

With -v ARR, every ready fd is stored as a sparse array entry ARR[FD]=events,
where events is a token string (r/w/p/h/e/t). The fd is the array index, so
'${!ARR[@]}' lists the ready fds and ARR[$fd] gives their readiness.

-t SECS sets a timeout (duration strings like '1.5', '500ms' are accepted);
without -t it blocks forever. Returns success on readiness or timeout (0 ready
fds), failure only on error. Without -v no array is populated.

Examples:
   L_builtin epoll wait -v ready $ep
   for fd in "${!ready[@]}"; do
       echo "fd $fd: ${ready[$fd]}"
   done
   L_builtin epoll wait -t 2.5 -v r $ep   # timeout after 2.5s
```

#### `L_builtin epoll close`

```
L_builtin epoll close: usage: close EPOLLFD

Close the epoll instance file descriptor EPOLLFD via close(2). Use this to
release an epoll fd created with `create`.

Examples:
   L_builtin epoll close $ep
```

### `L_builtin ext`

```
L_builtin ext: usage: L_builtin ext [-h] <subcommand> [args ...]
Available subcommands:
  asort           asort [-nr] array ...  or  asort [-nr] -i dest source
  basename        basename string [suffix]
  cat             cat [-] [file ...]
  chmod           chmod [-R] mode file [file...]
  csv             csv [-a ARRAY] string
  cut             cut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] [file ...]
  dirname         dirname string
  dsv             dsv [-a ARRAYNAME] [-d DELIMS] [-Sgp] string
  enable_mypid    enable_mypid N
  false           false
  fdflags         fdflags [-v] [-s flags_string] [fd ...]
  finfo           finfo [-acdgiflmnopsuACGMPU] file [file...]
  fltexpr         fltexpr [-p] expression
  getconf         getconf -[ah] [file] or getconf [-v spec] sysvar or getconf [-v spec] pathvar pathname
  head            head [-n num] [file ...]
  hello           hello
  id              id [user]  id -G [-n] [user]  id -g [-nr] [user]  id -u [-nr] [user]
  kv              kv [-A ARRAYNAME] [-s SEPARATORS] [-d RS]
  lcut            lcut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] line
  ln              ln [-fhns] file1 [file2] OR ln [-fhns] file ... directory
  logname         logname
  mkdir           mkdir [-p] [-m mode] directory [directory ...]
  mkfifo          mkfifo [-m mode] fifo_name [fifo_name ...]
  mktemp          mktemp [-d] [-q] [-t prefix] [-u] [-v varname] [template] ...
  echo            echo [args]
  pathchk         pathchk [-p] pathname ...
  print           print [-Rnprs] [-u unit] [-f format] [arguments]
  printenv        printenv [varname]
  push            push
  realpath        realpath [-a varname] [-cqsv] pathname [pathname...]
  rm              rm [-rf] file ...
  rmdir           rmdir directory ...
  seq             seq [-f format] [-s separator] [-w] [FIRST [INCR]] LAST
  setpgid         setpgid pid pgrpid
  sleep           sleep seconds[.fraction]
  stat            stat [-lL] [-A aname] file
  strftime        strftime format [seconds]
  strptime        strptime [-f format] date-time
  sync            sync [file ...]
  tee             tee [-ai] [file ...]
  template        template
  true            true
  tty             tty [-s]
  uname           uname [-amnrsv]
  unlink          unlink name
  whoami          whoami
```

#### `L_builtin ext asort`

```
L_builtin ext asort: usage: asort [-nr] array ...  or  asort [-nr] -i dest source

Sort arrays in-place.

Options:
  -n  compare according to string numerical value
  -r  reverse the result of comparisons
  -i  sort using indices/keys

If -i is supplied, SOURCE is not sorted in-place, but the indices (or keys
if associative) of SOURCE, after sorting it by its values, are placed as
values in the indexed array DEST

Associative arrays may not be sorted in-place.

Exit status:
Return value is zero unless an error happened (like invalid variable name
or readonly array).
```

#### `L_builtin ext basename`

```
L_builtin ext basename: usage: basename string [suffix]

Return non-directory portion of pathname.

The STRING is converted to a filename corresponding to the last
pathname component in STRING.  If the suffix string SUFFIX is
supplied, it is removed.
```

#### `L_builtin ext cat`

```
L_builtin ext cat: usage: cat [-] [file ...]

Display files.

Read each FILE and display it on the standard output.   If any
FILE is `-' or if no FILE argument is given, the standard input
is read.
```

#### `L_builtin ext chmod`

```
L_builtin ext chmod: usage: chmod [-R] mode file [file...]

Change file mode bits.

Change file mode bits.  Change the mode bits of files named as
arguments, in the order specified, as specified by MODE.The MODE argument may be an octal number or a symbolic mode like
that described in chmod(1).  If a symbolic mode is used, the
operations are interpreted relative to an initial mode of "a=rwx".

The return value is 0 unless an error occurs.
```

#### `L_builtin ext csv`

```
L_builtin ext csv: usage: csv [-a ARRAY] string

Read comma-separated fields from a string.

Parse STRING, a line of comma-separated values, into individual fields,
and store them into the indexed array ARRAYNAME starting at index 0.
If ARRAYNAME is not supplied, "CSV" is the default array name.
```

#### `L_builtin ext cut`

```
L_builtin ext cut: usage: cut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] [file ...]

Extract selected fields from each line of a file.

Select portions of each line (as specified by LIST) from each FILE
and write them to the standard output, or assign them to the indexed
array ARRAY starting at index 0. cut reads from the standard
input if no FILE arguments are specified or if a FILE argument is a
single hyphen.

Items specified by LIST are either column positions or fields delimited
by a special character, and are described more completely in cut(1).

Columns correspond to bytes (-b), characters (-c), or fields (-f). The
field delimiter is specified by -d (default TAB). Column numbering
starts at 1.

When -a is specified, cut assigns the output from each line it
processes to successive elements of ARRAY, beginning at 0. The
strings cut assigns to ARRAY are identical to the strings it would
write to the standard output if -a were not supplied.
```

#### `L_builtin ext dirname`

```
L_builtin ext dirname: usage: dirname string

Display directory portion of pathname.

The STRING is converted to the name of the directory containing
the filename corresponding to the last pathname component in STRING.
```

#### `L_builtin ext dsv`

```
L_builtin ext dsv: usage: dsv [-a ARRAYNAME] [-d DELIMS] [-Sgp] string

Read delimiter-separated fields from STRING.

Parse STRING, a line of delimiter-separated values, into individual
fields, and store them into the indexed array ARRAYNAME starting at
index 0. The parsing understands and skips over double-quoted strings. 
If ARRAYNAME is not supplied, "DSV" is the default array name.
If the delimiter is a comma, the default, this parses comma-
separated values as specified in RFC 4180.

The -d option specifies the delimiter. The delimiter is the first
character of the DELIMS argument. Specifying a DELIMS argument that
contains more than one character is not supported and will produce
unexpected results. The -S option enables shell-like quoting: double-
quoted strings can contain backslashes preceding special characters,
and the backslash will be removed; and single-quoted strings are
processed as the shell would process them. The -g option enables a
greedy split: sequences of the delimiter are skipped at the beginning
and end of STRING, and consecutive instances of the delimiter in STRING
do not generate empty fields. If the -p option is supplied, dsv leaves
quote characters as part of the generated field; otherwise they are
removed.

The return value is 0 unless an invalid option is supplied or the ARRAYNAME
argument is invalid or readonly.
```

#### `L_builtin ext enable_mypid`

```
L_builtin ext enable_mypid: usage: enable_mypid N

Enable $MYPID.

Enables use of the ${MYPID} dynamic variable.  
It will yield the current pid of a subshell.
```

#### `L_builtin ext false`

```
L_builtin ext false: usage: false

Exit unsuccessfully.

Return an unsuccessful result.
```

#### `L_builtin ext fdflags`

```
L_builtin ext fdflags: usage: fdflags [-v] [-s flags_string] [fd ...]

Display and modify file descriptor flags.

Display or, if the -s option is supplied, set flags for each file
descriptor supplied as an argument.  If the -v option is supplied,
the display is verbose, including each settable option name in the
form of a string such as that accepted by the -s option.

The -s option accepts a string with a list of flag names, each preceded
by a `+' (set) or `-' (unset).  Those changes are applied to each file
descriptor supplied as an argument.

If no file descriptor arguments are supplied, the displayed information
consists of the status of flags for each of the shell's open files.
```

#### `L_builtin ext finfo`

```
L_builtin ext finfo: usage: finfo [-acdgiflmnopsuACGMPU] file [file...]

Display information about file attributes.

Display information about each FILE.  Only single operators should
be supplied.  If no options are supplied, a summary of the info
available about each FILE is printed.  If FILE is of the form
/dev/fd/XX, file descriptor XX is described.  Operators, if supplied,
have the following meanings:

	-a	last file access time
	-A	last file access time in ctime format
	-c	last file status change time
	-C	last file status change time in ctime format
	-m	last file modification time
	-M	last file modification time in ctime format
	-d	device
	-i	inode
	-f	composite file identifier (device:inode)
	-g	gid of owner
	-G	group name of owner
	-l	name of file pointed to by symlink
	-n	link count
	-o	permissions in octal
	-p	permissions in ascii
	-P mask permissions ANDed with MASK (like with umask)
	-s	file size in bytes
	-u	uid of owner
	-U	user name of owner
```

#### `L_builtin ext fltexpr`

```
L_builtin ext fltexpr: usage: fltexpr [-p] expression

Evaluate floating-point arithmetic expression.

Evaluate EXPRESSION as a floating-point arithmetic expression and,
if the -p option is supplied, print the value to the standard output.

Exit Status:
If the EXPRESSION evaluates to 0, the return status is 1; 0 otherwise.
```

#### `L_builtin ext getconf`

```
L_builtin ext getconf: usage: getconf -[ah] [file] or getconf [-v spec] sysvar or getconf [-v spec] pathvar pathname

Display values of system limits and options.

getconf writes the current value of a configurable system limit or
option variable to the standard output.
```

#### `L_builtin ext head`

```
L_builtin ext head: usage: head [-n num] [file ...]

Display lines from beginning of file.

Copy the first N lines from the input files to the standard output.
N is supplied as an argument to the `-n' option.  If N is not given,
the first ten lines are copied.
```

#### `L_builtin ext hello`

```
L_builtin ext hello: usage: hello

Sample builtin.

this is the long doc for the sample hello builtin
```

#### `L_builtin ext id`

```
L_builtin ext id: usage: id [user]
	id -G [-n] [user]
	id -g [-nr] [user]
	id -u [-nr] [user]

Display information about user.
Return information about user identity
```

#### `L_builtin ext kv`

```
L_builtin ext kv: usage: kv [-A ARRAYNAME] [-s SEPARATORS] [-d RS]

Read key-value pairs into an associative array.

Read delimiter-terminated records composed of a single key-value pair
from the standard input and add the key and corresponding value
to the associative array ARRAYNAME. The key and value are separated
by a sequence of one or more characters in SEPARATORS. Records are
terminated by the first character of RS, similar to the read and
mapfile builtins.

If SEPARATORS is not supplied, $IFS is used to separate the keys
and values. If RS is not supplied, newlines terminate records.
If ARRAYNAME is not supplied, "KV" is the default array name.

Returns success if at least one key-value pair is stored in ARRAYNAME.
```

#### `L_builtin ext lcut`

```
L_builtin ext lcut: usage: lcut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] line

Extract selected fields from a string.

Select portions of LINE (as specified by LIST) and assign them to
element 0 of the indexed array ARRAY, or write them to the standard
output if -a is not specified.

Items specified by LIST are either column positions or fields delimited
by a special character, and are described more completely in cut(1).

Columns correspond to bytes (-b), characters (-c), or fields (-f). The
field delimiter is specified by -d (default TAB). Column numbering
starts at 1.

When -a is specified, lcut assigns the selected portions of LINE
to index 0 of ARRAY. The string lcut assigns to ARRAY is identical
to the string it would write to the standard output if -a were not
supplied.
```

#### `L_builtin ext ln`

```
L_builtin ext ln: usage: ln [-fhns] file1 [file2] OR ln [-fhns] file ... directory

Link files.

Create a new directory entry with the same modes as the original
file.  The -f option means to unlink any existing file, permitting
the link to occur.  The -s option means to create a symbolic link.
By default, ln makes hard links.  Specifying -n or its synonym -h
causes ln to not resolve symlinks in the target file or directory.
```

#### `L_builtin ext logname`

```
L_builtin ext logname: usage: logname

Display user login name.

Write the current user's login name to the standard output
and exit.  logname ignores the LOGNAME and USER variables.
logname ignores any non-option arguments.
```

#### `L_builtin ext mkdir`

```
L_builtin ext mkdir: usage: mkdir [-p] [-m mode] directory [directory ...]

Create directories.

Make directories.  Create the directories named as arguments, in
the order specified, using mode rwxrwxrwx as modified by the current
umask (see `help umask').  The -m option causes the file permission
bits of the final directory to be MODE.  The MODE argument may be
an octal number or a symbolic mode like that used by chmod(1).  If
a symbolic mode is used, the operations are interpreted relative to
an initial mode of "a=rwx".  The -p option causes any required
intermediate directories in PATH to be created.  The directories
are created with permission bits of rwxrwxrwx as modified by the current
umask, plus write and search permissions for the owner.  mkdir
returns 0 if the directories are created successfully, and non-zero
if an error occurs.
```

#### `L_builtin ext mkfifo`

```
L_builtin ext mkfifo: usage: mkfifo [-m mode] fifo_name [fifo_name ...]

Create FIFOs (named pipes).

Make FIFOs.  Create the FIFOs named as arguments, in
the order specified, using mode a=rw as modified by the current
umask (see `help umask').  The -m option causes the file permission
bits of the final FIFO to be MODE.  The MODE argument may be
an octal number or a symbolic mode like that used by chmod(1).  If
a symbolic mode is used, the operations are interpreted relative to
an initial mode of "a=rw".  mkfifo returns 0 if the FIFOs are
umask, plus write and search permissions for the owner.  mkdir
created successfully, and non-zero if an error occurs.
```

#### `L_builtin ext mktemp`

```
L_builtin ext mktemp: usage: mktemp [-d] [-q] [-t prefix] [-u] [-v varname] [template] ...

Make unique temporary file name

Take each supplied filename template and overwrite a portion of it
to create a filename, which is unique and may be used by the calling
script. TEMPLATE is a string ending in some number of 'X's. If
TEMPLATE is not supplied, shtmp.XXXXXX is used and $TMPDIR is used as
the name of the containing directory. Files are created u+rw; directories
are created u+rwx.

Options, if supplied, have the following meanings:

    -d    Create a directory instead of a file
    -q    Do not print error messages about file creation failure
    -t PREFIX Use PREFIX as the directory in which to create files
    -u    Do not create anything; simply print a name
    -v VAR    Store the generated name into shell variable VAR

Any PREFIX supplied with -t is ignored if TEMPLATE is supplied.

The return status is true if the file or directory was created successfully;
false if an error occurs or VAR is invalid or readonly.
```

#### `L_builtin ext echo`

```
/home/kamil/myprojects/L_builtin/build/bash/system/bash: line 2: L_builtin ext: unknown subcommand `echo'
L_builtin ext: usage: L_builtin ext [-h] <subcommand> [args ...]
Available subcommands:
  asort           asort [-nr] array ...  or  asort [-nr] -i dest source
  basename        basename string [suffix]
  cat             cat [-] [file ...]
  chmod           chmod [-R] mode file [file...]
  csv             csv [-a ARRAY] string
  cut             cut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] [file ...]
  dirname         dirname string
  dsv             dsv [-a ARRAYNAME] [-d DELIMS] [-Sgp] string
  enable_mypid    enable_mypid N
  false           false
  fdflags         fdflags [-v] [-s flags_string] [fd ...]
  finfo           finfo [-acdgiflmnopsuACGMPU] file [file...]
  fltexpr         fltexpr [-p] expression
  getconf         getconf -[ah] [file] or getconf [-v spec] sysvar or getconf [-v spec] pathvar pathname
  head            head [-n num] [file ...]
  hello           hello
  id              id [user]  id -G [-n] [user]  id -g [-nr] [user]  id -u [-nr] [user]
  kv              kv [-A ARRAYNAME] [-s SEPARATORS] [-d RS]
  lcut            lcut [-a ARRAY] [-b LIST] [-c LIST] [-f LIST] [-d CHAR] [-sn] line
  ln              ln [-fhns] file1 [file2] OR ln [-fhns] file ... directory
  logname         logname
  mkdir           mkdir [-p] [-m mode] directory [directory ...]
  mkfifo          mkfifo [-m mode] fifo_name [fifo_name ...]
  mktemp          mktemp [-d] [-q] [-t prefix] [-u] [-v varname] [template] ...
  echo            echo [args]
  pathchk         pathchk [-p] pathname ...
  print           print [-Rnprs] [-u unit] [-f format] [arguments]
  printenv        printenv [varname]
  push            push
  realpath        realpath [-a varname] [-cqsv] pathname [pathname...]
  rm              rm [-rf] file ...
  rmdir           rmdir directory ...
  seq             seq [-f format] [-s separator] [-w] [FIRST [INCR]] LAST
  setpgid         setpgid pid pgrpid
  sleep           sleep seconds[.fraction]
  stat            stat [-lL] [-A aname] file
  strftime        strftime format [seconds]
  strptime        strptime [-f format] date-time
  sync            sync [file ...]
  tee             tee [-ai] [file ...]
  template        template
  true            true
  tty             tty [-s]
  uname           uname [-amnrsv]
  unlink          unlink name
  whoami          whoami
```

#### `L_builtin ext pathchk`

```
L_builtin ext pathchk: usage: pathchk [-p] pathname ...

Check pathnames for validity.

Check each pathname argument for validity (i.e., it may be used to
create or access a file without causing syntax errors) and portability
(i.e., no filename truncation will result).  If the `-p' option is
supplied, more extensive portability checks are performed.
```

#### `L_builtin ext print`

```
L_builtin ext print: usage: print [-Rnprs] [-u unit] [-f format] [arguments]

Display arguments.

Output the arguments.  The -f option means to use the argument as a
format string as would be supplied to printf(1).  The rest of the
options are as in ksh.
```

#### `L_builtin ext printenv`

```
L_builtin ext printenv: usage: printenv [varname]

Display environment.

Print names and values of environment variables
```

#### `L_builtin ext push`

```
L_builtin ext push: usage: push

Create child shell.

Create a child that is an exact duplicate of the running shell
and wait for it to exit.  The $SHLVL, $!, $$, and $PPID variables
are adjusted in the child.  The return value is the exit status
of the child.
```

#### `L_builtin ext realpath`

```
L_builtin ext realpath: usage: realpath [-a varname] [-cqsv] pathname [pathname...]

Display pathname in canonical form.

Display the canonicalized version of each PATHNAME argument, resolving
symbolic links.
The -a option stores each canonicalized PATHNAME argument into the indexed
array VARNAME.
The -c option checks whether or not each resolved name exists.
The -q option produces no output; the exit status determines the
validity of each PATHNAME, but any array assignment is still performed.
If the -s option is supplied, canonicalize . and .. pathname components
without resolving symbolic links.
The -v option produces verbose output.
The exit status is 0 if each PATHNAME was resolved; non-zero otherwise.
```

#### `L_builtin ext rm`

```
L_builtin ext rm: usage: rm [-rf] file ...

Remove files.

rm removes the files specified as arguments.
```

#### `L_builtin ext rmdir`

```
L_builtin ext rmdir: usage: rmdir directory ...

Remove directory.

rmdir removes the directory entry specified by each argument,
provided the directory is empty.
```

#### `L_builtin ext seq`

```
L_builtin ext seq: usage: seq [-f format] [-s separator] [-w] [FIRST [INCR]] LAST

Print numbers from FIRST to LAST, in steps of INCREMENT.

-f FORMAT    use printf style floating-point FORMAT
-s STRING    use STRING to separate numbers (default: 
)
-w           equalize width by padding with leading zeroes

If FIRST or INCREMENT is omitted, it defaults to 1.  However, an
omitted INCREMENT defaults to -1 when LAST is smaller than FIRST.
The sequence of numbers ends when the sum of the current number and
INCREMENT would become greater than LAST.
FIRST, INCREMENT, and LAST are interpreted as floating point values.

FORMAT must be suitable for printing one argument of type 'double';
it defaults to %.PRECf if FIRST, INCREMENT, and LAST are all fixed point
decimal numbers with maximum precision PREC, and to %g otherwise.
```

#### `L_builtin ext setpgid`

```
L_builtin ext setpgid: usage: setpgid pid pgrpid

invoke the setpgid(2) system call

Arguments:
   pid : numeric process identifier, >= 0
   pgrpid: numeric process group identifier, >=0
See the setpgid(2) manual page.
```

#### `L_builtin ext sleep`

```
L_builtin ext sleep: usage: sleep seconds[.fraction]

Suspend execution for specified period.
sleep suspends execution for a minimum of SECONDS[.FRACTION] seconds.
As an extension, sleep accepts GNU-style time intervals (e.g., 2m30s).
```

#### `L_builtin ext stat`

```
L_builtin ext stat: usage: stat [-lL] [-A aname] file

Load an associative array with file status information.

Take a filename and load the status information returned by a
stat(2) call on that file into the associative array specified
by the -A option.  The default array name is STAT.

If the -L option is supplied, stat does not resolve symbolic links
and reports information about the link itself.  The -l option results
in longer-form listings for some of the fields. When -l is used,
the -F option supplies a format string passed to strftime(3) to
display the file time information.
The exit status is 0 unless the stat fails or assigning the array
is unsuccessful.
```

#### `L_builtin ext strftime`

```
L_builtin ext strftime: usage: strftime format [seconds]

Display formatted time.

Converts date and time format to a string and displays it on the
standard output.  If the optional second argument is supplied, it
is used as the number of seconds since the epoch to use in the
conversion, otherwise the current time is used.
```

#### `L_builtin ext strptime`

```
L_builtin ext strptime: usage: strptime [-f format] date-time

Convert a date-time string to seconds since the epoch.

Take DATE-TIME, a date-time string, and parse it using FORMAT, a
date and time format accepted by strptime(3). If FORMAT is not supplied,
attempt to parse DATE-TIME against a set of common date-time formats,
not all of which may be acceptable to strptime(3).
If the string matches one of the formats, convert it into seconds
since the epoch and display the result.
```

#### `L_builtin ext sync`

```
L_builtin ext sync: usage: sync [file ...]

Sync disks or specified files.

If one or more FILEs is supplied, force completion of pending writes
to those files. Otherwise, force completion of any pending disk
writes.

Exit Status: zero unless any FILE could not be synced.
```

#### `L_builtin ext tee`

```
L_builtin ext tee: usage: tee [-ai] [file ...]

Duplicate standard output.

Copy standard input to standard output, making a copy in each
filename argument.  If the `-a' option is given, the specified
files are appended to, otherwise they are overwritten.  If the
`-i' option is supplied, tee ignores interrupts.
```

#### `L_builtin ext template`

```
L_builtin ext template: usage: template

Short description.
Longer description of builtin and usage.
```

#### `L_builtin ext true`

```
L_builtin ext true: usage: true

Exit successfully.

Return a successful result.
```

#### `L_builtin ext tty`

```
L_builtin ext tty: usage: tty [-s]

Display terminal name.

tty writes the name of the terminal that is opened for standard
input to standard output.  If the `-s' option is supplied, nothing
is written; the exit status determines whether or not the standard
input is connected to a tty.
```

#### `L_builtin ext uname`

```
L_builtin ext uname: usage: uname [-amnrsv]

Display system information.

Display information about the system hardware and OS.
```

#### `L_builtin ext unlink`

```
L_builtin ext unlink: usage: unlink name

Remove a directory entry.

Forcibly remove a directory entry, even if it's a directory.
```

#### `L_builtin ext whoami`

```
L_builtin ext whoami: usage: whoami

Print user name

Display name of current user.
```

### `L_builtin fcntl`

```
L_builtin fcntl: usage: getfl [-v VAR] FD | setfl FD FLAGS | getfd [-v VAR] FD | setfd FD FLAGS | dup [-v VAR] [-c] FD [START]

Manipulate file descriptor properties via fcntl(2).

Subcommands:
  getfl [-v VAR] FD        Get file status flags (F_GETFL).  Without -v the
                           decoded flag names (e.g. 'nonblock,append') and the
                           raw value are printed.
  setfl FD FLAGS           Set file status flags (F_SETFL).  FLAGS is a
                           comma-separated list of open(2) flag names, e.g.
                           'nonblock,append' or an empty string to clear all
                           status flags.
  getfd [-v VAR] FD        Get file descriptor flags (F_GETFD).
  setfd FD FLAGS           Set file descriptor flags (F_SETFD).  FLAGS is a
                           comma-separated list of fd flag names (e.g.
                           'cloexec'), or an empty string to clear.
  dup [-v VAR] [-c] FD [START]
                           Duplicate FD via F_DUPFD.  START is the minimum fd
                           (default 0).  With -c, F_DUPFD_CLOEXEC is used
                           instead (close-on-exec is set on the new fd).

The file descriptor can be any open fd (as with the `close`, `lseek`,
`timerfd` and `signalfd` subcommands).

Exit Status:
  Returns success unless fcntl(2) fails, an unknown flag name is given, or
  the variable cannot be bound.

Examples:
  L_builtin fcntl getfl 3
  L_builtin fcntl setfl 3 nonblock,append
  L_builtin fcntl setfl 3 ''      # clear all status flags
  L_builtin fcntl getfd 3
  L_builtin fcntl setfd 3 cloexec
  L_builtin fcntl dup 3
  L_builtin fcntl dup -c 3 256    # new fd >= 256 with close-on-exec
  L_builtin fcntl getfl -v result 3
```

#### `L_builtin fcntl getfl`

```
L_builtin fcntl getfl: usage: getfl [-v VAR] FD

Read the file status flags of FD via fcntl(2) F_GETFL.

Without -v, the decoded flag names and raw integer value are printed to stdout.
With -v VAR, the raw integer value is stored in the shell variable VAR.

Examples:
  L_builtin fcntl getfl 3
  L_builtin fcntl getfl -v flags 3
```

#### `L_builtin fcntl setfl`

```
L_builtin fcntl setfl: usage: setfl FD FLAGS

Set the file status flags of FD via fcntl(2) F_SETFL.

FLAGS is a comma-separated list of open(2) flag names.  Any combination of
the following is accepted (availability depends on the platform):
  rdonly, wronly, rdwr, creat, excl, noctty, trunc, append, nonblock,
  ndelay, sync, dsync, rsync, async, direct, directory, nofollow, noatime,
  cloexec, path, tmpfile, largefile

An empty string clears all status flags.

Examples:
  L_builtin fcntl setfl 3 nonblock,append
  L_builtin fcntl setfl 3 ''
```

#### `L_builtin fcntl getfd`

```
L_builtin fcntl getfd: usage: getfd [-v VAR] FD

Read the file descriptor flags of FD via fcntl(2) F_GETFD.

Without -v, the decoded flag names and raw integer value are printed.
With -v VAR, the raw integer value is stored in VAR.

Examples:
  L_builtin fcntl getfd 3
  L_builtin fcntl getfd -v flags 3
```

#### `L_builtin fcntl setfd`

```
L_builtin fcntl setfd: usage: setfd FD FLAGS

Set the file descriptor flags of FD via fcntl(2) F_SETFD.

FLAGS is a comma-separated list of fd flag names.  Currently the only
supported flag is 'cloexec' (FD_CLOEXEC).  An empty string clears all
fd flags.

Examples:
  L_builtin fcntl setfd 3 cloexec
  L_builtin fcntl setfd 3 ''
```

#### `L_builtin fcntl dup`

```
L_builtin fcntl dup: usage: dup [-v VAR] [-c] FD [START]

Duplicate FD via fcntl(2) F_DUPFD (or F_DUPFD_CLOEXEC with -c).

START specifies the minimum file descriptor to allocate (default 0).
Without -v, the new fd is printed; with -v VAR it is stored in VAR.

Options:
  -c   Use F_DUPFD_CLOEXEC instead of F_DUPFD (the new fd has close-on-exec
      set).
  -v   Store the result in VAR instead of printing.

Examples:
  L_builtin fcntl dup 3
  L_builtin fcntl dup -c 3
  L_builtin fcntl dup -v newfd 3 256
```

### `L_builtin listen`

```
L_builtin listen: usage: [-p PORT_VAR] LISTENFD_VAR [IP] [PORT]

Create a new socket, bind it to IP and PORT, listen for incoming
connections, and store the resulting socket file descriptor in the
variable LISTENFD_VAR.

If IP is omitted, it defaults to 127.0.0.1.
If PORT is omitted, it defaults to 0 (ephemeral port allocation).

If -p PORT_VAR is provided, the actual bound port (useful when passing 0
for ephemeral port allocation) is stored in PORT_VAR.

Exit Status:
Returns success unless socket/bind/listen fails or variable binding fails.
```

### `L_builtin lua`

```
L_builtin lua: usage: <script> [args...]

Run a Lua script in-process, with access to a bash.* API.

Options:
  -h, --help          Show this help and exit

Arguments:
  script              Lua script: inline code, or a file path
  args...             Arguments exposed to the script via the Lua 'arg' table

bash.* API:

  bash.get(var_name [, base])
    Get a bash variable.
      var_name: string  -- variable name to retrieve
      base:     integer -- index base for arrays (0 or 1, default 1)
    Returns:
      scalar     -> string (or nil if unset)
      indexed    -> table with integer keys (shifted by base), sparse preserved
      associative-> table with string keys
      unset/nil  -> nil

  bash.set(var_name, value [, base])
    Set a bash variable.
      var_name: string          -- variable name to set
      value:    boolean|number|string|table
      base:     integer         -- index base for indexed arrays (0 or 1, default 1)
    Returns: boolean (true on success)
    Behavior:
      boolean  -> "true" or "false"
      number   -> decimal text (1.0 prints as "1")
      string   -> raw bytes
      table    -> if existing var is associative, or table has any string keys:
                     creates/sets associative array
                  else:
                     creates/sets indexed array (keys shifted by base)
    Errors on: readonly variables, invalid types, non-integer array keys

  bash.unset(var_name)
    Unset a bash variable.
      var_name: string
    Returns: boolean (true if removed, false if did not exist)
    Errors on: readonly variables

  bash.eval(command_string)
    Execute a command string using bash's parser (like 'eval').
      command_string: string
    Returns: integer exit status
    Note: Disabled on bash < 4.3 (missing l_execute_command_string)

  bash.expand(string)
    Perform bash parameter/command/arithmetic expansion on a string.
      string: string
    Returns: expanded string

  bash.expand_list(string)
    Expand a string using bash word expansion (glob, brace, tilde, etc.).
      string: string
    Returns: table (1-indexed) of expanded words

Lua 'arg' table:
  arg[1], arg[2], ...  -- script arguments (arg[0] is not set)

Examples:

  -- Get scalar
  local home = bash.get("HOME")
  print(home)

  -- Indexed array
  bash.set("MY_ARRAY", {"a", "b", "c"}, 0)  -- indices 0,1,2
  local arr = bash.get("MY_ARRAY")
  for i, v in ipairs(arr) do print(i, v) end
  local arr = bash.get("MY_ARRAY", 1)
  for i, v in ipairs(arr) do print(i, v) end

  -- Set scalar
  bash.set("FOO", "bar")
  bash.set("BOOL", true)
  bash.set("NUM", 42)

  -- Associative array
  bash.set("MY_ASSOC", {key1="val1", key2="val2"})
  local assoc = bash.get("MY_ASSOC")
  for k, v in pairs(assoc) do print(k, v) end

  -- Unset
  bash.unset("TEMP_VAR")

  -- Eval command
  local status = bash.eval("echo hello")

  -- Expand string
  local expanded = bash.expand("$HOME/.config")

  -- Expand list (glob, brace)
  local files = bash.expand_list("*.rs")
  for _, f in ipairs(files) do print(f) end
```

### `L_builtin memfd`

```
L_builtin memfd: usage: VAR [NAME]

Create an anonymous memory-backed file (memfd_create(2)) and store its file
descriptor in the shell variable VAR. The fd is a regular file-like object
living in RAM; its name appears in /proc/self/fd. NAME, if given, names the
memfd (otherwise a default name is used). The memfd is created with
MFD_CLOEXEC | MFD_NOEXEC_SEAL.

Exit Status:
  Returns success unless memfd_create fails or the variable cannot be bound.

Examples:
  // Create memfd with default name, store fd in MYFD
  L_builtin memfd MYFD

  // Create memfd named mydata, store fd in MYFD
  L_builtin memfd MYFD mydata

  // Use memfd as temporary in-RAM storage
  L_builtin memfd FD
  echo data >&$FD
  cat <&$FD
```

### `L_builtin mutex`

```
L_builtin mutex: usage: create [-n NAME] [-r] MUTEX | open MUTEX NAME | lock [-n] [-t SECS] MUTEX | unlock [-a] MUTEX | close MUTEX | destroy MUTEX

Process-shared mutual-exclusion lock backed by shared memory.

Subcommands:
  create [-n NAME] [-r] MUTEX
                            Create a mutex. MUTEX receives an opaque integer
                            handle (a bash variable). Without -n the mutex lives in
                            anonymous shared memory (shared across forked processes,
                            such as a background job started with &). With -n NAME it
                            is backed by a named shared-memory object (shm_open) that
                            unrelated processes can open. With -r the mutex is robust:
                            if the owning process terminates while holding it, the
                            next lock recovers instead of deadlocking forever.
  open MUTEX NAME           Open an existing named mutex NAME and assign its handle
                            to MUTEX.
  lock MUTEX [-t SECS] [-n] Acquire the lock. -t SECS sets a timeout in seconds
                            (e.g. 1.123); -n is non-blocking and returns immediately
                            (0 if acquired, non-zero if already held).
  unlock [-a] MUTEX         Release the lock. Fails if the current process does not
                            hold it. With -a, release every mutex this process
                            currently holds (ignoring MUTEX).
  close MUTEX               Unmap the mutex in the current process without destroying
                           the shared resource.
  destroy MUTEX             Unmap and, for a named mutex, unlink its shared-memory
                           object globally.

The bash variable holds only an opaque integer; the underlying shared-memory
pointer is never exposed.

Examples:
  alias m='L_builtin mutex'
  m create var
  ( m lock $var; echo locked; m unlock $var ) &
  m lock $var; m unlock $var
  m create -n /my_mutex v
  m open w /my_mutex
  m lock w -t 1.123
  m unlock v
  m destroy v
```

#### `L_builtin mutex create`

```
L_builtin mutex create: usage: create [-n NAME] [-r] MUTEX

Create a mutex and store its handle into the shell variable MUTEX.

Without -n the mutex is created in anonymous shared memory and is shared across
forked processes (for example a background job started with &). With -n NAME it
is backed by a named shared-memory object (shm_open) that unrelated processes can
later open. With -r the mutex is robust: if the owning process terminates while
holding it, the next lock recovers (instead of deadlocking forever) - the new
owner must still be prepared for possibly inconsistent shared state.

Examples:
  L_builtin mutex create var
  L_builtin mutex create -r var
  L_builtin mutex create -n /my_mutex v
  L_builtin mutex create -n -r /my_mutex v
```

#### `L_builtin mutex open`

```
L_builtin mutex open: usage: open MUTEX NAME

Open an existing named mutex NAME and assign its handle to MUTEX.

The named mutex must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin mutex open w /my_mutex
```

#### `L_builtin mutex lock`

```
L_builtin mutex lock: usage: lock [-n] [-t SECS] MUTEX

Acquire the lock MUTEX.

Options:
  -n        Non-blocking: return immediately, 0 if acquired, non-zero if already
            held.
  -t SECS   Timeout in seconds (e.g. 1.123); if the lock is not acquired within
            SECS, fail.

Examples:
  L_builtin mutex lock $var
  L_builtin mutex lock $var -n
  L_builtin mutex lock $var -t 1.123
```

#### `L_builtin mutex unlock`

```
L_builtin mutex unlock: usage: unlock [-a] MUTEX

Release the lock MUTEX. Fails if the current process does not hold the lock.

With -a, release every mutex this process currently holds, ignoring MUTEX. This
is useful as a cleanup at the end of a script.

Examples:
  L_builtin mutex unlock $var
  L_builtin mutex unlock -a
```

#### `L_builtin mutex close`

```
L_builtin mutex close: usage: close MUTEX

Unmap the mutex MUTEX in the current process without destroying the shared
resource. Other processes keep their mappings.

Examples:
  L_builtin mutex close $var
```

#### `L_builtin mutex destroy`

```
L_builtin mutex destroy: usage: destroy MUTEX

Destroy the mutex MUTEX: unmap it in the current process and, for a named mutex,
unlink its shared-memory object globally.

Examples:
  L_builtin mutex destroy $var
```

### `L_builtin pipe`

```
L_builtin pipe: usage: ARRAY

Create a new pipe and store the file descriptors in the indexed
array ARRAY. ARRAY[0] is the read end, ARRAY[1] is the write end.

Exit Status:
Returns success unless the pipe cannot be created or ARRAY is invalid.
```

### `L_builtin poll`

```
L_builtin poll: usage: [-t TIMEOUT] [-v ARRAY_VAR] [-i] [FD[:EVENTS] ...]

Wait for file descriptors to become ready.

L_builtin poll [-t TIMEOUT] [-v ARRAY_VAR] [-i] [FD[:EVENTS] ...]

Poll file descriptors using poll(2). EVENTS can be 'r', 'w', or 'p'.
Results are stored in the indexed array ARRAY_VAR as ARR[fd]=events: the fd
is the array index and the value is the decoded readiness tokens (e.g.
ARR[3]="r", ARR[5]="rw"). This sparse format matches the `epoll wait`
subcommand, so a readiness loop works against either.
REVENTS contains 'r', 'w', 'p', 'h' (hangup), 'e' (error), or 'n' 
(invalid).

If -i is provided, poll will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, poll retries on
EINTR.

Exit Status:
Returns success if poll succeeds, even if it timed out. Returns failure on
system errors.
```

### `L_builtin ppoll`

```
L_builtin poll: usage: [-t TIMEOUT] [-v ARRAY_VAR] [-u SIGSPEC] [-i] [FD[:EVENTS] ...]

Wait for file descriptors and unblock signals atomically.

L_builtin ppoll [-t TIMEOUT] [-v ARRAY_VAR] [-u SIGSPEC] [-i] [FD[:EVENTS] ...]

Poll file descriptors and unblock signals using ppoll(2).
Results are stored in the indexed array ARRAY_VAR as ARR[fd]=events: the fd
is the array index and the value is the decoded readiness tokens (e.g.
ARR[3]="r", ARR[5]="rw"). This sparse format matches the `epoll wait`
subcommand, so a readiness loop works against either.

Use -u SIGSPEC to temporarily unblock specified signals during ppoll.
Use -u 'ALL' (case-insensitive) to unblock all signals.

If -i is provided, ppoll will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, ppoll retries on
EINTR.

EVENTS and REVENTS format:
  EVENTS can be a combination of 'r' (read, default if omitted),
  'w' (write), or 'p' (priority).
  REVENTS contains 'r', 'w', 'p', 'h' (hangup), 'e' (error), or 'n' (invalid).

Example:
  # Poll fd 0 for reading with a 2.5 second timeout, unblocking all signals
  L_builtin ppoll -t 2.5 -v results -u ALL 0:r

Exit Status:
Returns success if ppoll succeeds. Returns failure on system errors.
```

### `L_builtin recv`

```
L_builtin recv: usage: [-f format] [-v RECV_VAR] [-n] [-i] FD SIZE

Receive up to SIZE bytes from the socket file descriptor FD.
Supported formats (-f):
  raw   Store raw bytes directly into RECV_VAR (null-byte unsafe) (default)
  hex   Store received bytes as hexadecimal string into RECV_VAR (null-byte safe)

If -n is provided, the recv call will be non-blocking. If no data is currently
available, it will return success immediately with an empty string.

If -i is provided, the recv call will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, recv retries on EINTR.

Exit Status:
Returns success unless recv fails or variable binding fails.
```

### `L_builtin semaphore`

```
L_builtin semaphore: usage: create [-n NAME] SEMAPHORE COUNT | open SEMAPHORE NAME | wait [-n] [-t SECS] SEMAPHORE | post SEMAPHORE | close SEMAPHORE | destroy SEMAPHORE

Process-shared counting semaphore backed by shared memory.

Subcommands:
  create [-n NAME] SEMAPHORE COUNT
                           Create a semaphore initialized to COUNT. SEMAPHORE
                           receives an opaque integer handle (a bash variable).
                           Without -n the semaphore lives in anonymous shared memory
                           (shared across forked processes, such as a background job
                           started with &; the counter is initialized via sem_init
                           with pshared=1). With -n NAME it is backed by a named
                           semaphore (sem_open) whose storage is owned by the kernel
                           and can be opened by unrelated processes.
  open SEMAPHORE NAME      Open an existing named semaphore NAME and assign its
                           handle to SEMAPHORE.
  wait SEMAPHORE [-t SECS] [-n]
                           Decrement the semaphore. -t SECS sets a timeout in seconds
                           (e.g. 1.123); -n is non-blocking and returns immediately (0
                           if decremented, non-zero if the count was 0).
  post SEMAPHORE           Increment the semaphore, waking a waiter if any.
  close SEMAPHORE          Release this process's reference without destroying the
                           shared resource.
  destroy SEMAPHORE        Destroy the semaphore; for a named semaphore, also unlink
                           its kernel object globally.

The bash variable holds only an opaque integer; the underlying shared-memory
pointer is never exposed.

Examples:
  alias s='L_builtin semaphore'
  s create var 1
  ( s wait $var; echo got; s post $var ) &
  s post $var
  s create -n /my_sem v 3
  s open w /my_sem
  s wait w -t 1.123
  s post v
  s destroy v
```

#### `L_builtin semaphore create`

```
L_builtin semaphore create: usage: create [-n NAME] SEMAPHORE COUNT

Create a semaphore initialized to COUNT and store its handle into the shell
variable SEMAPHORE.

Without -n the semaphore is created in anonymous shared memory and is shared
across forked processes (for example a background job started with &). With -n
NAME it is backed by a named semaphore (sem_open) whose storage is owned by the
kernel and can be opened by unrelated processes.

Examples:
  L_builtin semaphore create var 1
  L_builtin semaphore create -n /my_sem v 3
```

#### `L_builtin semaphore open`

```
L_builtin semaphore open: usage: open SEMAPHORE NAME

Open an existing named semaphore NAME and assign its handle to SEMAPHORE.

The named semaphore must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin semaphore open w /my_sem
```

#### `L_builtin semaphore wait`

```
L_builtin semaphore wait: usage: wait [-n] [-t SECS] SEMAPHORE

Decrement the semaphore SEMAPHORE.

Options:
  -n        Non-blocking: return immediately, 0 if decremented, non-zero if the
            count was 0.
  -t SECS   Timeout in seconds (e.g. 1.123); if the count is not positive within
            SECS, fail.

Examples:
  L_builtin semaphore wait $var
  L_builtin semaphore wait $var -n
  L_builtin semaphore wait $var -t 1.123
```

#### `L_builtin semaphore post`

```
L_builtin semaphore post: usage: post SEMAPHORE

Increment the semaphore SEMAPHORE, waking one waiter if any are blocked.

Examples:
  L_builtin semaphore post $var
```

#### `L_builtin semaphore close`

```
L_builtin semaphore close: usage: close SEMAPHORE

Release this process's reference to the semaphore without destroying the shared
resource. Other processes keep their references.

Examples:
  L_builtin semaphore close $var
```

#### `L_builtin semaphore destroy`

```
L_builtin semaphore destroy: usage: destroy SEMAPHORE

Destroy the semaphore: for an anonymous semaphore, destroy and unmap it; for a
named semaphore, close it and unlink its kernel object globally.

Examples:
  L_builtin semaphore destroy $var
```

### `L_builtin send`

```
L_builtin send: usage: [-f format] [-v SENT_VAR] [-n] FD DATA

Transmit raw or encoded data over the socket file descriptor FD.
Supported formats (-f):
  raw   Transmit DATA as raw characters (default)
  hex   Transmit DATA after decoding from hex representation

By default, send loops until all bytes are transmitted, retrying on short
writes and interrupted system calls (EINTR). If -n is provided, only a
single send(2) call is made and the result (which may be a short write)
is returned immediately.

If -v SENT_VAR is provided, the number of bytes successfully transmitted
is stored in SENT_VAR.

Exit Status:
Returns success unless send fails or variable binding fails.
```

### `L_builtin shutdown`

```
L_builtin shutdown: usage: FD [how]

Close parts or all of a full-duplex connection on network socket FD.
how can be one of:
  RD or 0    Further receptions will be disallowed
  WR or 1    Further transmissions will be disallowed
  RDWR or 2  Further receptions and transmissions will be disallowed (default)

Exit Status:
Returns success unless shutdown fails.
```

### `L_builtin sigmask`

```
L_builtin sigmask: usage: [-s sigspec] [-u sigspec] [sigspec ...]

Block or unblock signals.

L_builtin sigmask [-s sigspec] [-u sigspec] [sigspec ...]

Block or unblock signals in the shell process. Without options, it
prints the current signal mask. -s blocks, -u unblocks.
Use 'ALL' (case-insensitive) with -s or -u to block or unblock all
signals respectively. Positional arguments are always blocked.

Exit Status:
Returns success unless an invalid signal is provided or a system error occurs.
```

### `L_builtin sigunmask`

```
L_builtin sigunmask: usage: [-h] -s sigspec cmd [args...]

Unblock signals and run a command.

L_builtin sigunmask [-h] -s sigspec cmd [args...]

Temporarily unblocks the specified signal and executes the command.
Use 'ALL' (case-insensitive) with -s to unblock all signals.
If the signal was pending, the trap is executed and the command is skipped.
The command can be any shell command (builtin, function, or external).

WARNING: There is a small window between unblocking and starting the command.
If a signal arrives in this window, it may be delivered to the command itself
rather than being caught by this builtin's check.

Exit Status:
Returns the status of the command, or 128+signum if a signal was caught.
```

### `L_builtin sleep`

```
L_builtin sleep: usage: [-i] SECONDS

Sleep for the specified number of SECONDS. SECONDS can be a duration string
(e.g. `1s`, `500ms`, `1h30m`) or a floating-point number to request
sub-second/microsecond-level precision.

If -i is provided, the sleep will not automatically retry on signal interruption
(EINTR). Instead, it will fail with an error. By default, sleep retries on EINTR.

Exit Status:
  Returns success unless sleep fails.
```

### `L_builtin shm`

```
L_builtin shm: usage: add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME | rm [-s NAME | -n NAME | -f PATH] | unbind [-s NAME | -n NAME | -f PATH] VAR_NAME... | info [-s NAME | -n NAME | -f PATH] | ls [-s NAME | -n NAME | -f PATH] | sync [-s NAME | -n NAME | -f PATH] VAR_NAME

Shared-memory variables backed by a rkyv database.

Subcommands:
  add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME
                           Bind bash variable VAR_NAME (indexed, or associative
                           with -A) to a shared database. -s selects a POSIX
                           shared memory object named NAME; -n an anonymous
                           in-memory mapping (memfd) named NAME; -f a regular file
                           at PATH; with none the default in-memory mapping named
                           DEFAULT is used. The value is stored under VAR_NAME.
  rm [-s NAME | -n NAME | -f PATH]
                            Remove the whole database: unbind every variable bound
                            to it and unlink its backing object/file (for -s/-f).
  unbind [-s NAME | -n NAME | -f PATH] VAR_NAME...
                           Unbind the named variable(s) from this shell (drop the
                           registry entry and unbind the bash variable); does not
                           remove the data from the database.
  info [-s NAME | -n NAME | -f PATH]
                           Print every variable stored in the database (default:
                           the DEFAULT database).
  ls  [-s NAME | -n NAME | -f PATH]
                            List databases. With no flag, list every database this
                            session knows about with the variables bound to each;
                            with a backing flag, list only the variables bound to
                            that database in this session's REGISTRY.
  sync [-s NAME | -n NAME | -f PATH] VAR_NAME
                            Push the current bash variable values into the shared
                            database, replacing the variable's existing entry. The
                            variable must already be bound via 'add'.

The variable (indexed or associative array) is serialized into a rkyv blob on
every assignment and is visible to every process that maps the same database
 (e.g. a background job started with &, when using -s or -f or -n).
```

#### `L_builtin shm add`

```
L_builtin shm add: usage: add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME

Bind bash variable VAR_NAME (indexed, or associative with -A) to a shared
database.

The database is selected by one of:
  -s NAME   a POSIX shared memory object (shm_open) named NAME;
  -n NAME   an anonymous in-memory mapping (memfd_create) named NAME;
  -f PATH   a regular file at PATH (a path on a disc);
  neither   the default in-memory mapping named DEFAULT.
Every assignment is written through to the blob and is visible to every process
that maps the same database, e.g. a background job started with & (for -s/-f/-n).

With -A, create an associative array (key-value pairs with string keys) instead
of an indexed array (integer indices). NAME (for -s/-n) must be a valid shell
variable name; -f takes a path.

Examples:
  L_builtin shm add v
  v=(a b c)          # default in-memory mapping 'DEFAULT', shared with forked children
  v[0]=changed       # a single-index write is visible to other processes

  L_builtin shm add -s mydb v
  v=(a b c)          # POSIX shared memory 'mydb', shared across processes

  L_builtin shm add -f /tmp/mydb v
  v=(a b c)          # regular file at /tmp/mydb

  L_builtin shm add -A -s mydb v
  v=( [foo]=bar [baz]=qux )  # associative array in shared memory 'mydb'
```

#### `L_builtin shm rm`

```
L_builtin shm rm: usage: rm [-s NAME | -n NAME | -f PATH]

Remove the whole shared database: unbind every variable this shell has bound to
it, drop the registry entries, and unlink the backing object/file (for -s/-f).

The database is selected by the same -s/-n/-f flags as 'add'; with none given,
the default 'DEFAULT' database is removed.

Examples:
  L_builtin shm rm -s mydb   # remove shared memory 'mydb' entirely
  L_builtin shm rm -n mymem  # remove the in-memory mapping 'mymem'
  L_builtin shm rm          # remove the default 'DEFAULT' database
```

#### `L_builtin shm unbind`

```
L_builtin shm unbind: usage: unbind [-s NAME | -n NAME | -f PATH] VAR_NAME [VAR_NAME...]

Unbind the named variable(s) from this shell: drop the registry entry and unbind
the bash variable. This does NOT remove the variable's data from the shared
database; another process that has the variable bound may still read it.

The database is selected by the same -s/-n/-f flags as 'add'; with none given,
the default 'DEFAULT' database is used.

Examples:
  L_builtin shm unbind -s mydb v   # stop sharing 'v' from shared memory 'mydb'
  L_builtin shm unbind v w         # unbind 'v' and 'w' from the default database
```

#### `L_builtin shm info`

```
L_builtin shm info: usage: info [-s NAME | -n NAME | -f PATH]

Print every variable stored in a shared-memory database.

The database is selected by the same -s/-n/-f flags as 'add' (default: the
'DEFAULT' database). The output is a series of bash array assignments, one per
variable, that can be eval'd to reconstruct the shared state.

Examples:
  L_builtin shm info -s mydb
```

#### `L_builtin shm ls`

```
L_builtin shm ls: usage: ls [-s NAME | -n NAME | -f PATH]

List databases. With no flag, list every database this shell session currently
knows about, together with the bash variables bound to each. With a backing flag,
list only the variables bound to that database in this session's REGISTRY.

Databases are shown by their backing kind and name: 'shm:NAME' for POSIX shared
memory, 'memfd:NAME' for in-memory, and the file path for -f databases.
```

#### `L_builtin shm sync`

```
L_builtin shm sync: usage: sync [-s NAME | -n NAME | -f PATH] VAR_NAME

Push the current bash variable values into the shared database, replacing the
variable's existing entry. The variable must already be bound to the database
via 'add'. For each element in the bash array (indexed or associative), the
current value is written to the database.

Normally the dynamic setter (invoked on each element assignment) keeps the
database in sync automatically. However, a bulk array reassignment such as
v=( new1 new2 new3 ) only triggers the setter for the new elements -- stale
elements from the previous array are not removed from the database. 'sync'
is useful for propagating structural changes (element deletion, array
shrinking) or for explicitly committing the current state after a batch of
operations.

Examples:
   L_builtin shm add -s mydb v
   v=( a b c )
   L_builtin shm sync -s mydb v       # push v=(a b c) into shared mem 'mydb'
```

### `L_builtin splice`

```
L_builtin splice: usage: [-v BYTES_VAR] FD_IN FD_OUT LEN [FLAGS]

Move up to LEN bytes from FD_IN to FD_OUT without copying them through
userspace (splice(2)). At least one fd must be a pipe. The number of bytes
moved is stored in BYTES_VAR (or printed if -v is omitted).

FLAGS combos:
  move   SPLICE_F_MOVE
  nonblock  SPLICE_F_NONBLOCK
  more   SPLICE_F_MORE
  gift   SPLICE_F_GIFT

Exit Status:
Returns success unless splice fails.

Examples:
  // Splice 1024 bytes from fd 3 (pipe) to fd 4 (pipe), print bytes moved
  L_builtin splice 3 4 1024

  // Splice with nonblock flag, store bytes moved in MOVED
  L_builtin splice -v MOVED 3 4 4096 nonblock

  // Splice with multiple flags (comma-separated)
  L_builtin splice 3 4 8192 move,more

  // Typical use: zero-copy pipe-to-pipe transfer
  // (assuming fd 3 is readable pipe, fd 4 is writable pipe)
  L_builtin splice 3 4 65536

  // Copy file to pipe (fd 3=file, fd 4=pipe) - requires splice support
  L_builtin splice 3 4 1048576
```

### `L_builtin signalfd`

```
L_builtin signalfd: usage: [-n] [-b] [-v FD_VAR] [SIGNAL...]

Create a signalfd(2) and store its file descriptor in FD_VAR (or print it if
-v is omitted). The fd becomes readable whenever one of the listed SIGNALs is
pending, so signals can be polled as an fd - see also the `poll` subcommand.

SIGNAL names (SIGTERM, INT, HUP, ...) or numbers are accepted. If none are
given, the fd covers every signal.

Options:
  -n     SFD_NONBLOCK
  -b     Also block (sigprocmask) the listed signals so they are consumed
         by reads from the fd instead of running their default action
  -v     Store the resulting fd in the variable FD_VAR

Exit Status:
Returns success unless signalfd fails or the variable cannot be bound.
```

### `L_builtin timerfd`

```
L_builtin timerfd: usage: create [-c CLOCK] [-s SEC] [-i SEC] [-n] FD_VAR | set FD [-s SEC] [-i SEC] [-c CLOCK]

Create a timerfd(2) and arm it, or modify an existing timerfd's settings.

SEC accepts a duration string (e.g. `1s`, `500ms`, `1h30m`) or a bare
floating-point number interpreted as seconds.

Subcommands:
  create [-c CLOCK] [-s SEC] [-i SEC] [-n] FD_VAR
                        Create a timerfd(2) and store its file descriptor in
                        the shell variable FD_VAR. The fd becomes readable when
                        the timer expires, so it can be polled together with
                        other fds - see also the `poll`/`ppoll` subcommands.
                        -c     CLOCK (CLOCK_REALTIME or CLOCK_MONOTONIC;
                               default CLOCK_MONOTONIC)
                        -s     Initial expiry as a duration string or (possibly
                                fractional) seconds; default 0 = do not arm
                        -i     Periodic interval as a duration string or (possibly
                                fractional) seconds; default 0
                        -n     TFD_NONBLOCK

  set FD [-s SEC] [-i SEC] [-c CLOCK]
                        Read the current timer settings with timerfd_gettime,
                        change -s (initial expiry) and/or -i (interval) as
                        given, then re-arm with timerfd_settime. At least one
                        of -s/-i is required. CLOCK is accepted for
                        compatibility but must match the fd's clock.

Exit Status:
  Returns success unless timerfd_create/timerfd_settime fails or the variable
  cannot be bound.
```

#### `L_builtin timerfd create`

```
L_builtin timerfd create: usage: create [-c CLOCK] [-s SEC] [-i SEC] [-n] [-v FD_VAR]

Create a timerfd(2) and store its file descriptor in FD_VAR (or print it if
-v is omitted). The fd becomes readable when the timer expires, so it can be
polled together with other fds - see also the `poll`/`ppoll` subcommands.

Options:
  -c     CLOCK (CLOCK_REALTIME or CLOCK_MONOTONIC; default CLOCK_MONOTONIC)
  -s     Initial expiry as a duration string or (possibly fractional) seconds; default 0 = do not arm
  -i     Periodic interval as a duration string or (possibly fractional) seconds; default 0
  -n     TFD_NONBLOCK

Examples:
   L_builtin timerfd create -n -s 0.5 tf
   L_builtin timerfd create -s 1.0 -i 0.25 tf
```

### `L_builtin lseek`

```
L_builtin lseek: usage: [-v var] fd offset [whence]

Adjust the file offset of file descriptor FD to OFFSET bytes
according to WHENCE.

WHENCE can be one of:
  0 or SET  Seek from the beginning (default)
  1 or CUR  Seek from the current position
  2 or END  Seek from the end

If -v VAR is provided, the new offset is stored in VAR.

Exit Status:
Returns success unless an error occurs during lseek or variable binding.
```

### `L_builtin barrier`

```
L_builtin barrier: usage: create [-n NAME] BARRIER COUNT | open BARRIER NAME | wait BARRIER [-t SECS] [-n] | close BARRIER | reset BARRIER | destroy BARRIER

Process synchronization barriers backed by shared memory.

Subcommands:
  create [-n NAME] BARRIER COUNT
                          Create a barrier for COUNT processes. BARRIER receives an
                          opaque integer handle. Without -n the barrier lives in
                          anonymous shared memory (shared across forked processes,
                          such as a background job started with &). With -n NAME
                          it is backed by a named shared-memory object (shm_open)
                          that unrelated processes can open.
  open BARRIER NAME           Open an existing named barrier NAME and assign its
                          handle to BARRIER.
  wait BARRIER [-t SECS] [-n]  Block until the barrier is satisfied. -t SECS sets a
                          timeout in seconds (e.g. 1.123); -n is non-blocking and
                          returns immediately (0 if satisfied, non-zero if not).
  close BARRIER               Unmap the barrier in the current process without
                          destroying the shared resource.
  reset BARRIER               Reset the barrier for reuse (clears the satisfied state
                          and the arrival count).
  destroy BARRIER             Unmap and, for a named barrier, unlink its shared-memory
                          object globally.

The bash variable holds only an opaque integer; the underlying shared-memory
pointer is never exposed.

Examples:
  alias b='L_builtin barrier'
  b create var 2
  ( b wait $var; echo waited ) &
  b wait $var; echo also waited
  b create -n /my_barrier v 3
  b open w /my_barrier
  b wait w -t 1.123
  b reset v
  b destroy v
```

#### `L_builtin barrier create`

```
L_builtin barrier create: usage: create [-n NAME] BARRIER COUNT

Create a barrier for COUNT processes.

BARRIER receives an opaque integer handle (a bash variable). Without -n the barrier
is created in anonymous shared memory and is shared across forked processes
(for example a background job started with &). With -n NAME it is backed by a
named shared-memory object (shm_open) that unrelated processes can later open.

Examples:
  L_builtin barrier create var 2
  L_builtin barrier create -n /my_barrier v 3
```

#### `L_builtin barrier open`

```
L_builtin barrier open: usage: open BARRIER NAME

Open an existing named barrier NAME and assign its handle to BARRIER.

The named barrier must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin barrier open w /my_barrier
```

#### `L_builtin barrier wait`

```
L_builtin barrier wait: usage: wait [-t SECS] [-n] BARRIER

Wait until the barrier BARRIER is satisfied.

Options:
  -t SECS   Timeout in seconds (e.g. 1.123); if the barrier is not satisfied
            within SECS, fail.
  -n        Non-blocking: return immediately, 0 if the barrier is already
            satisfied, non-zero otherwise.

Examples:
  L_builtin barrier wait $var
  L_builtin barrier wait -t 1.123 $var
  L_builtin barrier wait -n $var
```

#### `L_builtin barrier close`

```
L_builtin barrier close: usage: close BARRIER

Unmap the barrier BARRIER in the current process without destroying the shared
resource. Other processes keep their mappings.

Examples:
  L_builtin barrier close $var
```

#### `L_builtin barrier reset`

```
L_builtin barrier reset: usage: reset BARRIER

Reset the barrier BARRIER for reuse: clears the satisfied state and the arrival
count so a fresh round can begin.

Examples:
  L_builtin barrier reset $var
```

#### `L_builtin barrier destroy`

```
L_builtin barrier destroy: usage: destroy BARRIER

Destroy the barrier BARRIER: unmap it in the current process and, for a named
barrier, unlink its shared-memory object globally.

Examples:
  L_builtin barrier destroy $var
```

<!-- README_GEN_END -->

## License



This project is licensed under the GNU General Public License v3.0 - see [LICENSE](LICENSE) for details.



## Self promotion



[mkdocstrings-sh](https://github.com/kamilcuk/mkdocstrings-sh), [L_lib](https://github.com/Kamilcuk/L_lib), [L_bash_profile](https://github.com/Kamilcuk/L_bash_profile).

