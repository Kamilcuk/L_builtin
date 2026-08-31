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

use std::ffi::CStr;
use std::os::raw::{c_char, c_int};

use cmdargs_derive::CmdArgs;

use crate::bash_api::{Cpnt, EXECUTION_FAILURE, EX_USAGE, WORD_LIST};
use crate::cmdargs::BashVar;
use crate::handles::HandleEntry;
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};
use crate::{
    handles::{map_anonymous, map_named, unmap, HandleRegistry},
    l_builtin_error,
    shared::timespec_from_now,
};

thread_local! {
    /// Handle registry for mutexes.
    pub(crate) static MUTEX_REGISTRY: HandleRegistry = HandleRegistry::new();
}

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
///
/// `robust`: additionally set `PTHREAD_MUTEX_ROBUST` so that if the owning
/// process terminates while holding the lock, the next `lock` observes
/// `EOWNERDEAD` (and we recover via `pthread_mutex_consistent`) instead of
/// deadlocking forever.
unsafe fn mutex_init(b: *mut Mutex, robust: bool) -> Result<(), String> {
    let mut attr: libc::pthread_mutexattr_t = std::mem::zeroed();
    if libc::pthread_mutexattr_init(&mut attr) != 0 {
        return Err("pthread_mutexattr_init failed".into());
    }
    libc::pthread_mutexattr_setpshared(&mut attr, libc::PTHREAD_PROCESS_SHARED);
    libc::pthread_mutexattr_settype(&mut attr, libc::PTHREAD_MUTEX_ERRORCHECK);
    if robust {
        if libc::pthread_mutexattr_setrobust(&mut attr, libc::PTHREAD_MUTEX_ROBUST) != 0 {
            libc::pthread_mutexattr_destroy(&mut attr);
            return Err("pthread_mutexattr_setrobust failed".into());
        }
    }
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
///
/// A robust mutex whose owner died yields `EOWNERDEAD`; we mark it consistent
/// and treat the lock as acquired.
unsafe fn mutex_lock(b: *mut Mutex, timeout: Option<f64>, nonblock: bool) -> CmdResult {
    let m = &mut (*b).mtx;
    let rc = if nonblock {
        libc::pthread_mutex_trylock(m)
    } else if let Some(secs) = timeout {
        let ts = timespec_from_now(secs);
        libc::pthread_mutex_timedlock(m, &ts)
    } else {
        libc::pthread_mutex_lock(m)
    };
    match rc {
        0 => Ok(()),
        rc if rc == libc::EOWNERDEAD => {
            if libc::pthread_mutex_consistent(m) == 0 {
                Ok(())
            } else {
                l_builtin_error!(b"failed to recover inconsistent mutex");
                Err(EXECUTION_FAILURE)
            }
        }
        _ => Err(EXECUTION_FAILURE),
    }
}

unsafe fn mutex_unlock(b: *mut Mutex) -> CmdResult {
    if libc::pthread_mutex_unlock(&mut (*b).mtx) == 0 {
        Ok(())
    } else {
        Err(EXECUTION_FAILURE)
    }
}

/// `L_builtin mutex create [-n NAME] [-r] MUTEX`
#[derive(CmdArgs)]
struct MutexCreateArgs {
    /// Robust mutex: recover instead of deadlocking if the owner dies.
    #[flag('r')]
    robust: bool,
    /// Named shared-memory object (shm_open) rather than anonymous memory.
    #[opt('n')]
    name: Option<*const c_char>,
    /// Shell variable receiving the opaque handle.
    #[positional]
    mutex: BashVar,
}

pub unsafe fn mutex_create_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_CREATE_CMD.enter();
    let args = MutexCreateArgs::parse(list)?;
    let name = args.name.map(|p| unsafe { CStr::from_ptr(p) }.to_owned());
    let size = mutex_bytes();
    let ptr = if let Some(n) = &name {
        match map_named(n, size, true) {
            Ok(p) => p,
            Err(e) => {
                l_builtin_error!(e.as_bytes());
                return Err(EXECUTION_FAILURE);
            }
        }
    } else {
        match map_anonymous(size) {
            Ok(p) => p,
            Err(e) => {
                l_builtin_error!(e.as_bytes());
                return Err(EXECUTION_FAILURE);
            }
        }
    };
    if let Err(e) = mutex_init(ptr as *mut Mutex, args.robust) {
        l_builtin_error!(e.as_bytes());
        unmap(ptr, size);
        return Err(EXECUTION_FAILURE);
    }
    let id = MUTEX_REGISTRY.with(|m| m.store(ptr, name));
    args.mutex.set_int(id)
}

/// `L_builtin mutex open MUTEX NAME`
#[derive(CmdArgs)]
struct MutexOpenArgs {
    /// Shell variable receiving the opaque handle.
    #[positional]
    mutex: BashVar,
    /// Name of the existing shared-memory mutex object.
    #[positional]
    name: *const c_char,
}

pub unsafe fn mutex_open_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_OPEN_CMD.enter();
    let args = MutexOpenArgs::parse(list)?;
    let name_c = Cpnt::new(args.name as *mut c_char).as_cstr().to_owned();
    let size = mutex_bytes();
    let ptr = match map_named(&name_c, size, false) {
        Ok(p) => p,
        Err(e) => {
            l_builtin_error!(e.as_bytes());
            return Err(EXECUTION_FAILURE);
        }
    };
    let id = MUTEX_REGISTRY.with(|m| m.store(ptr, Some(name_c)));
    args.mutex.set_int(id)?;
    Ok(())
}

/// `L_builtin mutex lock [-n] [-t SECS] MUTEX`
#[derive(CmdArgs)]
struct MutexLockArgs {
    /// Non-blocking: return immediately (0 if acquired, non-zero if busy).
    #[flag('n')]
    nonblock: bool,
    /// Timeout in seconds.
    #[opt('t')]
    timeout: Option<f64>,
    /// Opaque mutex handle value.
    #[positional]
    mutex: u64,
}

pub unsafe fn mutex_lock_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_LOCK_CMD.enter();
    let args = MutexLockArgs::parse(list)?;
    let ptr = match lookup_mutex(args.mutex) {
        Some(p) => p,
        None => {
            l_builtin_error!(b"unknown mutex handle");
            return Err(EXECUTION_FAILURE);
        }
    };
    mutex_lock(ptr, args.timeout, args.nonblock)
}

/// `L_builtin mutex unlock [-a] MUTEX`
#[derive(CmdArgs)]
struct MutexUnlockArgs {
    /// Unlock every held mutex, ignoring MUTEX.
    #[flag('a')]
    all: bool,
    /// Opaque mutex handle value.
    #[optional]
    mutex: Option<u64>,
}

pub unsafe fn mutex_unlock_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_UNLOCK_CMD.enter();
    let args = MutexUnlockArgs::parse(list)?;
    if args.all {
        MUTEX_REGISTRY.with(|m| {
            m.for_each(|_id, ptr| {
                let _ = mutex_unlock(ptr as *mut Mutex);
            });
        });
        Ok(())
    } else {
        let id = match args.mutex {
            Some(v) => v,
            None => {
                l_builtin_error!(b"missing MUTEX (or use -a to unlock all held mutexes)");
                return Err(EX_USAGE);
            }
        };
        let ptr = match lookup_mutex(id) {
            Some(p) => p,
            None => {
                l_builtin_error!(b"unknown mutex handle");
                return Err(EXECUTION_FAILURE);
            }
        };
        mutex_unlock(ptr)
    }
}

fn get_mutex_handle(mutex: u64) -> Result<HandleEntry, c_int> {
    match MUTEX_REGISTRY.with(|m| m.take(mutex)) {
        Some(e) => Ok(e),
        None => {
            l_builtin_error!(b"unknown mutex handle");
            Err(EXECUTION_FAILURE)
        }
    }
}

/// `L_builtin mutex close MUTEX`
#[derive(CmdArgs)]
struct MutexCloseArgs {
    /// Opaque mutex handle value.
    #[positional]
    mutex: u64,
}

pub unsafe fn mutex_close_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_CLOSE_CMD.enter();
    let args = MutexCloseArgs::parse(list)?;
    let entry = get_mutex_handle(args.mutex)?;
    unmap(entry.ptr, mutex_bytes());
    Ok(())
}

/// `L_builtin mutex destroy MUTEX`
#[derive(CmdArgs)]
struct MutexDestroyArgs {
    /// Opaque mutex handle value.
    #[positional]
    mutex: u64,
}

pub unsafe fn mutex_destroy_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_DESTROY_CMD.enter();
    let args = MutexDestroyArgs::parse(list)?;
    let entry = get_mutex_handle(args.mutex)?;
    unmap(entry.ptr, mutex_bytes());
    if let Some(n) = entry.name {
        unsafe { libc::shm_unlink(n.as_ptr()) };
    }
    Ok(())
}

fn lookup_mutex(id: u64) -> Option<*mut Mutex> {
    MUTEX_REGISTRY
        .with(|m| m.lookup(id))
        .map(|p| p as *mut Mutex)
}

const MUTEX_CMD: CmdDesc = CmdDesc::new(
    c"mutex",
    c"create [-n NAME] [-r] MUTEX | open MUTEX NAME | lock [-n] [-t SECS] MUTEX | unlock [-a] MUTEX | close MUTEX | destroy MUTEX",
    c"\
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
",
);

const MUTEX_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-n NAME] [-r] MUTEX",
    c"\
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
    c"unlock [-a] MUTEX",
    c"\
Release the lock MUTEX. Fails if the current process does not hold the lock.

With -a, release every mutex this process currently holds, ignoring MUTEX. This
is useful as a cleanup at the end of a script.

Examples:
  L_builtin mutex unlock $var
  L_builtin mutex unlock -a
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
pub unsafe fn mutex_subcommand(list: *mut WORD_LIST) -> CmdResult {
    MUTEX_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(MUTEX_TABLE)?;
    caller.call()
}
