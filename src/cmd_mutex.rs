//! L_builtin `mutex` subcommand: a process-shared mutual-exclusion lock backed
//! by shared memory.
//!
//! A mutex is created in shared memory - either anonymous shared memory
//! (`MAP_ANONYMOUS | MAP_SHARED`, shared across forked processes such as a `&`
//! background job) or a named shared-memory object created with `shm_open`
//! (shared across unrelated processes). The lock uses `pthread_mutex_t` with
//! `PTHREAD_PROCESS_SHARED` and the `PTHREAD_MUTEX_ERRORCHECK` type, so a lock
//! by a non-owner or a double-unlock fails cleanly instead of causing undefined
//! behavior.
//!
//! The bash variable holds only an opaque integer handle. Internally the handle
//! maps to `<void* mmap pointer, Option<shm name>>`; the raw pointer is never
//! exposed to the user. Only `create`/`open` take the variable *name* (they
//! assign the handle into it); `lock`/`unlock`/`close`/`destroy` take the
//! integer *value*.

use std::ffi::CString;
use std::os::raw::c_int;

use crate::bash_api::{
    this_cmd_name, WordListView, EXECUTION_FAILURE, EXECUTION_SUCCESS,
    EX_USAGE, WORD_LIST,
};
use crate::subcmd::{CmdDesc, SubcommandFn};
use crate::{
    beprintln, getopts, l_builtin_error,
    shared::{
        bind_handle, map_anonymous, map_named, parse_int, store_handle, take_handle,
        timespec_from_now, unmap, HANDLE_KIND_MUTEX,
    },
    subcmd_getopts,
};

/// Cross-process mutex laid out in shared memory.
///
/// The mutex is initialized (by the creator) with `PTHREAD_PROCESS_SHARED` so
/// it works across processes that map the same memory.
#[repr(C)]
struct Mutex {
    mtx: libc::pthread_mutex_t,
}

fn mutex_bytes() -> usize {
    std::mem::size_of::<Mutex>()
}

/// Initialize a mutex (creator only) with process-shared, error-checking attr.
unsafe fn mutex_init(b: *mut Mutex) -> Result<(), String> {
    let mut attr: libc::pthread_mutexattr_t = std::mem::zeroed();
    if libc::pthread_mutexattr_init(&mut attr) != 0 {
        return Err("pthread_mutexattr_init failed".into());
    }
    libc::pthread_mutexattr_setpshared(&mut attr, libc::PTHREAD_PROCESS_SHARED);
    libc::pthread_mutexattr_settype(&mut attr, libc::PTHREAD_MUTEX_ERRORCHECK);
    let rc = libc::pthread_mutex_init(&mut (*b).mtx, &attr);
    libc::pthread_mutexattr_destroy(&mut attr);
    if rc != 0 {
        return Err(format!("pthread_mutex_init failed: {}", rc));
    }
    Ok(())
}

/// Acquire the lock.
///
/// `nonblock`: `pthread_mutex_trylock` - 0 if acquired, non-zero if busy.
/// `timeout` (seconds): `pthread_mutex_timedlock` against a `CLOCK_REALTIME`
/// absolute deadline. Otherwise a blocking `pthread_mutex_lock`.
unsafe fn mutex_lock(b: *mut Mutex, timeout: Option<f64>, nonblock: bool) -> c_int {
    let m = &mut (*b).mtx;
    if nonblock {
        return if libc::pthread_mutex_trylock(m) == 0 {
            EXECUTION_SUCCESS
        } else {
            EXECUTION_FAILURE
        };
    }
    if let Some(secs) = timeout {
        let ts = timespec_from_now(secs);
        return if libc::pthread_mutex_timedlock(m, &ts) == 0 {
            EXECUTION_SUCCESS
        } else {
            EXECUTION_FAILURE
        };
    }
    if libc::pthread_mutex_lock(m) == 0 {
        EXECUTION_SUCCESS
    } else {
        EXECUTION_FAILURE
    }
}

unsafe fn mutex_unlock(b: *mut Mutex) -> c_int {
    if libc::pthread_mutex_unlock(&mut (*b).mtx) == 0 {
        EXECUTION_SUCCESS
    } else {
        EXECUTION_FAILURE
    }
}

pub unsafe extern "C" fn mutex_create_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut name: Option<CString> = None;
    let (var,) = subcmd_getopts!(
        MUTEX_CREATE_CMD,
        list,
        options: [ n => |nm| name = Some(unsafe { nm.as_cstr() }.to_owned()) ],
        required: [ MUTEX ],
    );
    let size = mutex_bytes();
    let ptr = if let Some(n) = &name {
        match map_named(n, size, true) {
            Ok(p) => p,
            Err(e) => {
                l_builtin_error!(e.as_bytes());
                return EXECUTION_FAILURE;
            }
        }
    } else {
        match map_anonymous(size) {
            Ok(p) => p,
            Err(e) => {
                l_builtin_error!(e.as_bytes());
                return EXECUTION_FAILURE;
            }
        }
    };
    if let Err(e) = mutex_init(ptr as *mut Mutex) {
        l_builtin_error!(e.as_bytes());
        unmap(ptr, size);
        return EXECUTION_FAILURE;
    }
    let id = store_handle(HANDLE_KIND_MUTEX, ptr, name);
    bind_handle(&var, id)
}

pub unsafe extern "C" fn mutex_open_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var, name) = subcmd_getopts!(
        MUTEX_OPEN_CMD,
        list,
        required: [ MUTEX, NAME ],
    );
    let name_c = unsafe { name.as_cstr() }.to_owned();
    let size = mutex_bytes();
    let ptr = match map_named(&name_c, size, false) {
        Ok(p) => p,
        Err(e) => {
            l_builtin_error!(e.as_bytes());
            return EXECUTION_FAILURE;
        }
    };
    let id = store_handle(HANDLE_KIND_MUTEX, ptr, Some(name_c));
    bind_handle(&var, id)
}

pub unsafe extern "C" fn mutex_lock_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut timeout: Option<f64> = None;
    let mut nonblock = false;
    let (var,) = subcmd_getopts!(
        MUTEX_LOCK_CMD,
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
        required: [ MUTEX ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    let ptr = match lookup_mutex(id) {
        Some(p) => p,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    mutex_lock(ptr, timeout, nonblock)
}

pub unsafe extern "C" fn mutex_unlock_subcommand(list: *mut WORD_LIST) -> c_int {
    MUTEX_UNLOCK_CMD.enter();
    let (var,) = subcmd_getopts!(
        MUTEX_UNLOCK_CMD,
        list,
        required: [ MUTEX ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    let ptr = match lookup_mutex(id) {
        Some(p) => p,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    mutex_unlock(ptr)
}

pub unsafe extern "C" fn mutex_close_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var,) = subcmd_getopts!(
        MUTEX_CLOSE_CMD,
        list,
        required: [ MUTEX ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    let entry = match take_handle(HANDLE_KIND_MUTEX, id) {
        Some(e) => e,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    unmap(entry.ptr, mutex_bytes());
    EXECUTION_SUCCESS
}

pub unsafe extern "C" fn mutex_destroy_subcommand(list: *mut WORD_LIST) -> c_int {
    let (var,) = subcmd_getopts!(
        MUTEX_DESTROY_CMD,
        list,
        required: [ MUTEX ],
    );
    let id = match parse_int::<u64>(var.as_ptr()) {
        Some(v) => v,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    let entry = match take_handle(HANDLE_KIND_MUTEX, id) {
        Some(e) => e,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return EXECUTION_FAILURE;
        }
    };
    unmap(entry.ptr, mutex_bytes());
    if let Some(n) = entry.name {
        unsafe { libc::shm_unlink(n.as_ptr()) };
    }
    EXECUTION_SUCCESS
}

fn lookup_mutex(id: u64) -> Option<*mut Mutex> {
    crate::shared::lookup_handle(HANDLE_KIND_MUTEX, id).map(|p| p as *mut Mutex)
}

const MUTEX_CMD: CmdDesc = CmdDesc::new(
    c"mutex",
    c"create [-n NAME] MUTEX | open MUTEX NAME | lock [-n] [-t SECS] MUTEX | unlock MUTEX | close MUTEX | destroy MUTEX",
    c"\
Process-shared mutual-exclusion lock backed by shared memory.

Subcommands:
  create [-n NAME] MUTEX    Create a mutex. MUTEX receives an opaque integer
                           handle (a bash variable). Without -n the mutex lives in
                           anonymous shared memory (shared across forked processes,
                           such as a background job started with &). With -n NAME it
                           is backed by a named shared-memory object (shm_open) that
                           unrelated processes can open.
  open MUTEX NAME           Open an existing named mutex NAME and assign its handle
                           to MUTEX.
  lock MUTEX [-t SECS] [-n] Acquire the lock. -t SECS sets a timeout in seconds
                           (e.g. 1.123); -n is non-blocking and returns immediately
                           (0 if acquired, non-zero if already held).
  unlock MUTEX              Release the lock. Fails if the current process does not
                           hold it.
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
",
);

const MUTEX_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-n NAME] MUTEX",
    c"\
Create a mutex and store its handle into the shell variable MUTEX.

Without -n the mutex is created in anonymous shared memory and is shared across
forked processes (for example a background job started with &). With -n NAME it
is backed by a named shared-memory object (shm_open) that unrelated processes can
later open.

Examples:
  L_builtin mutex create var
  L_builtin mutex create -n /my_mutex v
",
);

const MUTEX_OPEN_CMD: CmdDesc = CmdDesc::new(
    c"open",
    c"open MUTEX NAME",
    c"\
Open an existing named mutex NAME and assign its handle to MUTEX.

The named mutex must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin mutex open w /my_mutex
",
);

const MUTEX_LOCK_CMD: CmdDesc = CmdDesc::new(
    c"lock",
    c"lock [-n] [-t SECS] MUTEX",
    c"\
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
",
);

const MUTEX_UNLOCK_CMD: CmdDesc = CmdDesc::new(
    c"unlock",
    c"unlock MUTEX",
    c"\
Release the lock MUTEX. Fails if the current process does not hold the lock.

Examples:
  L_builtin mutex unlock $var
",
);

const MUTEX_CLOSE_CMD: CmdDesc = CmdDesc::new(
    c"close",
    c"close MUTEX",
    c"\
Unmap the mutex MUTEX in the current process without destroying the shared
resource. Other processes keep their mappings.

Examples:
  L_builtin mutex close $var
",
);

const MUTEX_DESTROY_CMD: CmdDesc = CmdDesc::new(
    c"destroy",
    c"destroy MUTEX",
    c"\
Destroy the mutex MUTEX: unmap it in the current process and, for a named mutex,
unlink its shared-memory object globally.

Examples:
  L_builtin mutex destroy $var
",
);

const MUTEX_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", mutex_create_subcommand),
    ("open", mutex_open_subcommand),
    ("lock", mutex_lock_subcommand),
    ("unlock", mutex_unlock_subcommand),
    ("close", mutex_close_subcommand),
    ("destroy", mutex_destroy_subcommand),
];

const MUTEX_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 6> =
    crate::intlookup!(&MUTEX_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn mutex_subcommand(list: *mut WORD_LIST) -> c_int {
    MUTEX_CMD.enter();
    let rest = getopts!(list, [], []);
    let mut iter = WordListView::from_raw(rest).into_iter();
    let action = match iter.next() {
        Some(a) => a,
        None => {
            beprintln!(
                this_cmd_name(),
                b": usage: L_builtin mutex <create|open|lock|unlock|close|destroy> ..."
            );
            return EX_USAGE;
        }
    };
    let action_bytes = unsafe { action.as_bytes() };
    let handler = match MUTEX_TABLE.lookup(action_bytes) {
        Some(h) => h,
        None => {
            beprintln!(
                this_cmd_name(),
                b": unknown mutex subcommand: ",
                action_bytes
            );
            return EX_USAGE;
        }
    };
    handler(iter.as_ptr())
}
