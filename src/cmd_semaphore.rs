//! L_builtin `semaphore` subcommand: a process-shared counting semaphore backed
//! by shared memory.
//!
//! A semaphore is created in shared memory - either anonymous shared memory
//! (`MAP_ANONYMOUS | MAP_SHARED`, shared across forked processes such as a `&`
//! background job, with the counter initialized via `sem_init(..., 1, value)`)
//! or a named semaphore created with `sem_open` (shared across unrelated
//! processes; the kernel owns its storage). `wait` decrements (blocking, with an
//! optional timeout, or non-blocking); `post` increments and wakes a waiter.
//!
//! The bash variable holds only an opaque integer handle. Internally the handle
//! maps to `<void* sem pointer, Option<name>>`; the raw pointer is never exposed
//! to the user. Only `create`/`open` take the variable *name* (they assign the
//! handle into it); `wait`/`post`/`close`/`destroy` take the integer *value*.

use std::ffi::CString;
use std::os::raw::{c_int, c_uint};

use crate::bash_api::{
    this_cmd_name, WordListView, EXECUTION_FAILURE, EXECUTION_SUCCESS,
    EX_USAGE, WORD_LIST,
};
use crate::subcmd::{CmdDesc, SubcommandFn};
use crate::{
    beprintln, getopts, l_builtin_error,
    shared::{
        bind_handle, lookup_handle, map_anonymous, parse_int, store_handle, take_handle,
        timespec_from_now, unmap, HANDLE_KIND_SEMAPHORE,
    },
    subcmd_getopts,
};

/// Anonymous semaphore laid out in shared memory. For a named semaphore the
/// registry stores the opaque `sem_t*` returned by `sem_open` instead.
#[repr(C)]
struct Semaphore {
    sem: libc::sem_t,
}

fn semaphore_bytes() -> usize {
    std::mem::size_of::<Semaphore>()
}

/// Wait (decrement) on the semaphore.
///
/// `nonblock`: `sem_trywait` - 0 if decremented, non-zero if the count was 0.
/// `timeout` (seconds): `sem_timedwait` against a `CLOCK_REALTIME` absolute
/// deadline. Otherwise a blocking `sem_wait`.
unsafe fn semaphore_wait(sem: *mut libc::sem_t, timeout: Option<f64>, nonblock: bool) -> c_int {
    if nonblock {
        return if libc::sem_trywait(sem) == 0 {
            EXECUTION_SUCCESS
        } else {
            EXECUTION_FAILURE
        };
    }
    if let Some(secs) = timeout {
        let ts = timespec_from_now(secs);
        return if libc::sem_timedwait(sem, &ts) == 0 {
            EXECUTION_SUCCESS
        } else {
            EXECUTION_FAILURE
        };
    }
    if libc::sem_wait(sem) == 0 {
        EXECUTION_SUCCESS
    } else {
        EXECUTION_FAILURE
    }
}

unsafe fn semaphore_post(sem: *mut libc::sem_t) -> c_int {
    if libc::sem_post(sem) == 0 {
        EXECUTION_SUCCESS
    } else {
        EXECUTION_FAILURE
    }
}

/// Tear down a registry entry: `sem_destroy`+`unmap` for anonymous, `sem_close`
/// (and optionally `sem_unlink`) for named.
unsafe fn semaphore_teardown(ptr: *mut u8, name: Option<CString>, unlink: bool) {
    let sem = ptr as *mut libc::sem_t;
    if let Some(n) = name {
        libc::sem_close(sem);
        if unlink {
            libc::shm_unlink(n.as_ptr());
        }
    } else {
        libc::sem_destroy(sem);
        unmap(ptr, semaphore_bytes());
    }
}

pub unsafe extern "C" fn semaphore_create_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut name: Option<CString> = None;
    let (var, count) = subcmd_getopts!(
        SEMAPHORE_CREATE_CMD,
        list,
        options: [ n => |nm| name = Some(unsafe { nm.as_cstr() }.to_owned()) ],
        required: [ SEMAPHORE, COUNT ],
    );
    let value = match parse_int::<c_uint>(count.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"invalid count");
            return EX_USAGE;
        }
    };
    let ptr = if let Some(n) = &name {
        let sem = libc::sem_open(n.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600, value);
        if sem.is_null() {
            l_builtin_error!(b"sem_open failed: ", std::io::Error::last_os_error());
            return EXECUTION_FAILURE;
        }
        sem as *mut u8
    } else {
        let p = match map_anonymous(semaphore_bytes()) {
            Ok(p) => p,
            Err(e) => {
                l_builtin_error!(e.as_bytes());
                return EXECUTION_FAILURE;
            }
        };
        if libc::sem_init(p as *mut libc::sem_t, 1, value) != 0 {
            l_builtin_error!(b"sem_init failed: ", std::io::Error::last_os_error());
            unmap(p, semaphore_bytes());
            return EXECUTION_FAILURE;
        }
        p
    };
    let id = store_handle(HANDLE_KIND_SEMAPHORE, ptr, name);
    bind_handle(&var, id)
}

pub unsafe extern "C" fn semaphore_open_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var, name) = subcmd_getopts!(
        SEMAPHORE_OPEN_CMD,
        list,
        required: [ SEMAPHORE, NAME ],
    );
    let name_c = unsafe { name.as_cstr() }.to_owned();
    let sem = libc::sem_open(name_c.as_ptr(), libc::O_RDWR, 0o600, 0);
    if sem.is_null() {
        l_builtin_error!(b"sem_open failed: ", std::io::Error::last_os_error());
        return EXECUTION_FAILURE;
    }
    let id = store_handle(HANDLE_KIND_SEMAPHORE, sem as *mut u8, Some(name_c));
    bind_handle(&var, id)
}

pub unsafe extern "C" fn semaphore_wait_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut timeout: Option<f64> = None;
    let mut nonblock = false;
    let (var,) = subcmd_getopts!(
        SEMAPHORE_WAIT_CMD,
        list,
        flags: [ n => || nonblock = true ],
        options: [ t => |tm| {
            match parse_int::<f64>(tm.as_ptr()) {
                Some(v) => {
                    timeout = Some(v);
                    true
                }
                None => false,
            }
        } ],
        required: [ SEMAPHORE ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    let ptr = match lookup_semaphore(id) {
        Some(p) => p,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    semaphore_wait(ptr, timeout, nonblock)
}

pub unsafe extern "C" fn semaphore_post_subcommand(list: *mut WORD_LIST) -> c_int {
    SEMAPHORE_POST_CMD.enter();
    let (var,) = subcmd_getopts!(
        SEMAPHORE_POST_CMD,
        list,
        required: [ SEMAPHORE ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    let ptr = match lookup_semaphore(id) {
        Some(p) => p,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    semaphore_post(ptr)
}

pub unsafe extern "C" fn semaphore_close_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var,) = subcmd_getopts!(
        SEMAPHORE_CLOSE_CMD,
        list,
        required: [ SEMAPHORE ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    let entry = match take_handle(HANDLE_KIND_SEMAPHORE, id) {
        Some(e) => e,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    unsafe { semaphore_teardown(entry.ptr, entry.name, false) };
    EXECUTION_SUCCESS
}

pub unsafe extern "C" fn semaphore_destroy_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var,) = subcmd_getopts!(
        SEMAPHORE_DESTROY_CMD,
        list,
        required: [ SEMAPHORE ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    let entry = match take_handle(HANDLE_KIND_SEMAPHORE, id) {
        Some(e) => e,
        None => {
            l_builtin_error!(b"unknown semaphore handle");
            return EXECUTION_FAILURE;
        }
    };
    unsafe { semaphore_teardown(entry.ptr, entry.name, true) };
    EXECUTION_SUCCESS
}

fn lookup_semaphore(id: u64) -> Option<*mut libc::sem_t> {
    lookup_handle(HANDLE_KIND_SEMAPHORE, id).map(|p| p as *mut libc::sem_t)
}

const SEMAPHORE_CMD: CmdDesc = CmdDesc::new(
    c"semaphore",
    c"create [-n NAME] SEMAPHORE COUNT | open SEMAPHORE NAME | wait [-n] [-t SECS] SEMAPHORE | post SEMAPHORE | close SEMAPHORE | destroy SEMAPHORE",
    c"\
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
",
);

const SEMAPHORE_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-n NAME] SEMAPHORE COUNT",
    c"\
Create a semaphore initialized to COUNT and store its handle into the shell
variable SEMAPHORE.

Without -n the semaphore is created in anonymous shared memory and is shared
across forked processes (for example a background job started with &). With -n
NAME it is backed by a named semaphore (sem_open) whose storage is owned by the
kernel and can be opened by unrelated processes.

Examples:
  L_builtin semaphore create var 1
  L_builtin semaphore create -n /my_sem v 3
",
);

const SEMAPHORE_OPEN_CMD: CmdDesc = CmdDesc::new(
    c"open",
    c"open SEMAPHORE NAME",
    c"\
Open an existing named semaphore NAME and assign its handle to SEMAPHORE.

The named semaphore must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin semaphore open w /my_sem
",
);

const SEMAPHORE_WAIT_CMD: CmdDesc = CmdDesc::new(
    c"wait",
    c"wait [-n] [-t SECS] SEMAPHORE",
    c"\
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
",
);

const SEMAPHORE_POST_CMD: CmdDesc = CmdDesc::new(
    c"post",
    c"post SEMAPHORE",
    c"\
Increment the semaphore SEMAPHORE, waking one waiter if any are blocked.

Examples:
  L_builtin semaphore post $var
",
);

const SEMAPHORE_CLOSE_CMD: CmdDesc = CmdDesc::new(
    c"close",
    c"close SEMAPHORE",
    c"\
Release this process's reference to the semaphore without destroying the shared
resource. Other processes keep their references.

Examples:
  L_builtin semaphore close $var
",
);

const SEMAPHORE_DESTROY_CMD: CmdDesc = CmdDesc::new(
    c"destroy",
    c"destroy SEMAPHORE",
    c"\
Destroy the semaphore: for an anonymous semaphore, destroy and unmap it; for a
named semaphore, close it and unlink its kernel object globally.

Examples:
  L_builtin semaphore destroy $var
",
);

const SEMAPHORE_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", semaphore_create_subcommand),
    ("open", semaphore_open_subcommand),
    ("wait", semaphore_wait_subcommand),
    ("post", semaphore_post_subcommand),
    ("close", semaphore_close_subcommand),
    ("destroy", semaphore_destroy_subcommand),
];

const SEMAPHORE_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 6> =
    crate::intlookup!(&SEMAPHORE_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn semaphore_subcommand(list: *mut WORD_LIST) -> c_int {
    SEMAPHORE_CMD.enter();
    let rest = getopts!(list, [], []);
    let mut iter = WordListView::from_raw(rest).into_iter();
    let action = match iter.next() {
        Some(a) => a,
        None => {
            beprintln!(
                this_cmd_name(),
                b": usage: L_builtin semaphore <create|open|wait|post|close|destroy> ..."
            );
            return EX_USAGE;
        }
    };
    let action_bytes = unsafe { action.as_bytes() };
    let handler = match SEMAPHORE_TABLE.lookup(action_bytes) {
        Some(h) => h,
        None => {
            beprintln!(
                this_cmd_name(),
                b": unknown semaphore subcommand: ",
                action_bytes
            );
            return EX_USAGE;
        }
    };
    handler(iter.as_ptr())
}
