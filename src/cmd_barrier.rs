//! L_builtin `barrier` subcommand: process synchronization barriers backed by
//! shared memory.
//!
//! A barrier is created in shared memory - either anonymous shared memory
//! (`MAP_ANONYMOUS | MAP_SHARED`, shared across forked processes such as a `&`
//! background job) or a named shared-memory object created with `shm_open`
//! (shared across unrelated processes). A barrier blocks until `COUNT` distinct
//! processes have called `wait` on it; once satisfied it stays satisfied until
//! explicitly `reset`.
//!
//! The bash variable holds only an opaque integer handle. Internally the handle
//! maps to `<void* mmap pointer, Option<shm name>>`; the raw pointer is never
//! exposed to the user.

use std::ffi::CString;
use std::os::raw::c_int;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    Cpnt, WordListIterCpnt, EXECUTION_FAILURE, EXECUTION_SUCCESS, EX_USAGE, WORD_LIST,
};
use crate::cmdargs::BashVar;
use crate::pthread::PthreadMutexGuard;
use crate::subcmd::{CmdDesc, SubcommandFn};
use crate::{
    handles::{map_anonymous, map_named, HandleRegistry},
    l_builtin_error,
    shared::timespec_from_now,
};

thread_local! {
    /// Handle registry for barriers.
    pub(crate) static BARRIER_REGISTRY: HandleRegistry = HandleRegistry::new();
}

/// Cross-process barrier laid out in shared memory.
///
/// The mutex and condvar are initialized (by the creator) with
/// `PTHREAD_PROCESS_SHARED` so they work across processes that map the same
/// memory. `count` is the number of processes that have arrived in the current
/// round, `target` is how many are required, and `satisfied` is sticky: it
/// becomes true once `target` arrivals are reached and stays true until `reset`
/// clears it.
#[repr(C)]
struct Barrier {
    mtx: libc::pthread_mutex_t,
    cond: libc::pthread_cond_t,
    count: u32,
    target: u32,
    satisfied: u32,
}

fn barrier_bytes() -> usize {
    std::mem::size_of::<Barrier>()
}

/// Initialize a barrier (creator only) with process-shared mutex/condvar.
unsafe fn barrier_init(b: *mut Barrier, target: u32) -> Result<(), String> {
    let bar = &mut *b;
    let mut mattr: libc::pthread_mutexattr_t = std::mem::zeroed();
    if libc::pthread_mutexattr_init(&mut mattr) != 0 {
        return Err("pthread_mutexattr_init failed".into());
    }
    libc::pthread_mutexattr_setpshared(&mut mattr, libc::PTHREAD_PROCESS_SHARED);
    let mut cattr: libc::pthread_condattr_t = std::mem::zeroed();
    if libc::pthread_condattr_init(&mut cattr) != 0 {
        libc::pthread_mutexattr_destroy(&mut mattr);
        return Err("pthread_condattr_init failed".into());
    }
    libc::pthread_condattr_setpshared(&mut cattr, libc::PTHREAD_PROCESS_SHARED);
    let rc = libc::pthread_mutex_init(&mut bar.mtx, &mattr);
    libc::pthread_mutexattr_destroy(&mut mattr);
    libc::pthread_condattr_destroy(&mut cattr);
    if rc != 0 {
        return Err(format!("pthread_mutex_init failed: {}", rc));
    }
    let rc = libc::pthread_cond_init(&mut bar.cond, &cattr);
    if rc != 0 {
        return Err(format!("pthread_cond_init failed: {}", rc));
    }
    bar.count = 0;
    bar.target = target;
    bar.satisfied = 0;
    Ok(())
}

/// Block (or poll) on a barrier until it is satisfied.
///
/// `nonblock`: return immediately - `Ok(true)` if the barrier is already
/// satisfied (and satisfying it if this call is the final arrival), `Ok(false)`
/// otherwise. `timeout` (seconds) applies only to blocking waits.
/// Record one arrival: increment the count, mark satisfied and broadcast once
/// `target` arrivals are reached. Returns `true` if the barrier is now satisfied.
unsafe fn barrier_arrive(bar: &mut Barrier) -> bool {
    bar.count += 1;
    if bar.count == bar.target {
        bar.satisfied = 1;
        libc::pthread_cond_broadcast(&mut bar.cond);
    }
    bar.satisfied != 0
}

unsafe fn barrier_wait(b: *mut Barrier, timeout: Option<f64>, nonblock: bool) -> c_int {
    let bar = &mut *b;
    let _guard = match PthreadMutexGuard::lock(&mut bar.mtx as *mut _) {
        Ok(g) => g,
        Err(_) => return EXECUTION_FAILURE,
    };
    if bar.satisfied != 0 {
        return EXECUTION_SUCCESS;
    }
    let ready = barrier_arrive(bar);
    if nonblock {
        return if ready {
            EXECUTION_SUCCESS
        } else {
            EXECUTION_FAILURE
        };
    }
    if ready {
        return EXECUTION_SUCCESS;
    }
    loop {
        let rc = match timeout {
            Some(secs) => {
                let ts = timespec_from_now(secs);
                libc::pthread_cond_timedwait(&mut bar.cond, &mut bar.mtx, &ts)
            }
            None => libc::pthread_cond_wait(&mut bar.cond, &mut bar.mtx),
        };
        if rc != 0 {
            return EXECUTION_FAILURE;
        }
        if bar.satisfied != 0 {
            break;
        }
    }
    EXECUTION_SUCCESS
}

/// Clear the satisfied state and arrival count so the barrier can be reused.
unsafe fn barrier_reset(b: *mut Barrier) -> c_int {
    let bar = &mut *b;
    let _guard = match PthreadMutexGuard::lock(&mut bar.mtx as *mut _) {
        Ok(g) => g,
        Err(_) => return EXECUTION_FAILURE,
    };
    bar.count = 0;
    bar.satisfied = 0;
    libc::pthread_cond_broadcast(&mut bar.cond);
    EXECUTION_SUCCESS
}

/// Store a barrier in the registry and return its opaque integer handle.
fn store_barrier(ptr: *mut u8, name: Option<CString>) -> u64 {
    BARRIER_REGISTRY.with(|r| r.store(ptr, name))
}

fn lookup_ptr(id: u64) -> Option<*mut Barrier> {
    match BARRIER_REGISTRY
        .with(|r| r.lookup(id))
        .map(|p| p as *mut Barrier)
    {
        Some(e) => Some(e),
        None => {
            l_builtin_error!(b"unknown barrier handle", id);
            None
        }
    }
}

/// Remove and return the registry entry (ptr + optional shm name).
fn take_barrier(id: u64) -> Option<(*mut Barrier, Option<CString>)> {
    match BARRIER_REGISTRY
        .with(|r| r.take(id))
        .map(|e| (e.ptr as *mut Barrier, e.name))
    {
        Some(e) => Some(e),
        None => {
            l_builtin_error!(b"unknown barrier handle", id);
            None
        }
    }
}

#[derive(CmdArgs)]
struct BarrierCreateSubcommand {
    /// Named shared-memory object (shm_open) rather than anonymous memory.
    #[opt('n')]
    name: Option<&'static CStr>,
    /// Shell variable receiving the opaque handle.
    #[positional]
    var: BashVar,
    /// Number of processes that must call wait.
    #[positional]
    count: u32,
}

pub unsafe extern "C" fn barrier_create_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_CREATE_CMD.enter();
    let args = match BarrierCreateSubcommand::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    if args.count == 0 {
        l_builtin_error!(b"count must be >= 1");
        return EX_USAGE;
    }
    let size = barrier_bytes();
    let ptr = if let Some(n) = &args.name {
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
    if let Err(e) = unsafe { barrier_init(ptr as *mut Barrier, args.count) } {
        l_builtin_error!(e.as_bytes());
        unsafe { libc::munmap(ptr as *mut libc::c_void, size) };
        return EXECUTION_FAILURE;
    }
    let id = store_barrier(
        ptr,
        match args.name {
            Some(x) => Some(x.to_owned()),
            None => None,
        },
    );
    match args.var.set_u64(id) {
        Ok(()) => EXECUTION_SUCCESS,
        Err(e) => return e,
    }
}

#[derive(CmdArgs)]
struct BarrierOpenArgs {
    /// Shell variable receiving the opaque handle.
    #[positional]
    barrier: BashVar,
    /// Name of the existing shared-memory barrier object.
    #[positional]
    name: *const c_char,
}

pub unsafe extern "C" fn barrier_open_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_OPEN_CMD.enter();
    let args = match BarrierOpenArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let name_c =
        unsafe { crate::bash_api::Cpnt::new(args.name as *mut c_char).as_cstr() }.to_owned();
    let size = barrier_bytes();
    let ptr = match map_named(&name_c, size, false) {
        Ok(p) => p,
        Err(e) => {
            l_builtin_error!(e.as_bytes());
            return EXECUTION_FAILURE;
        }
    };
    let id = store_barrier(ptr, Some(name_c));
    match args.barrier.set_u64(id) {
        Ok(()) => EXECUTION_SUCCESS,
        Err(e) => return e,
    }
}

#[derive(CmdArgs)]
struct BarrierWaitSubcommand {
    #[flag('n')]
    nonblock: bool,
    #[opt('t')]
    timeout: Option<f64>,
    #[positional]
    id: u64,
}

pub unsafe extern "C" fn barrier_wait_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_WAIT_CMD.enter();
    let args = match BarrierWaitSubcommand::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let ptr = match lookup_ptr(args.id) {
        Some(p) => p,
        None => return EXECUTION_FAILURE,
    };
    unsafe { barrier_wait(ptr, args.timeout, args.nonblock) }
}

#[derive(CmdArgs)]
struct BarrierCloseArgs {
    #[positional]
    id: u64,
}

pub unsafe extern "C" fn barrier_close_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_CLOSE_CMD.enter();
    let args = match BarrierCloseArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let entry = match take_barrier(args.id) {
        Some(e) => e,
        None => return EXECUTION_FAILURE,
    };
    let (ptr, _name) = entry;
    let size = barrier_bytes();
    unsafe { libc::munmap(ptr as *mut libc::c_void, size) };
    EXECUTION_SUCCESS
}

#[derive(CmdArgs)]
struct BarrierResetArgs {
    #[positional]
    barrier: u64,
}

pub unsafe extern "C" fn barrier_reset_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_RESET_CMD.enter();
    let args = match BarrierResetArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let ptr = match lookup_ptr(args.barrier) {
        Some(p) => p,
        None => return EXECUTION_FAILURE,
    };
    unsafe { barrier_reset(ptr) }
}

#[derive(CmdArgs)]
struct BarrierDestroyArgs {
    #[positional]
    barrier: u64,
}

pub unsafe extern "C" fn barrier_destroy_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_DESTROY_CMD.enter();
    let args = match BarrierDestroyArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let entry = match take_barrier(args.barrier) {
        Some(e) => e,
        None => return EXECUTION_FAILURE,
    };
    let (ptr, name) = entry;
    let size = barrier_bytes();
    unsafe {
        libc::munmap(ptr as *mut libc::c_void, size);
        if let Some(n) = name {
            libc::shm_unlink(n.as_ptr());
        }
    }
    EXECUTION_SUCCESS
}

const BARRIER_CMD: CmdDesc = CmdDesc::new(
    c"barrier",
    c"create [-n NAME] BARRIER COUNT | open BARRIER NAME | wait BARRIER [-t SECS] [-n] | close BARRIER | reset BARRIER | destroy BARRIER",
    c"\
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
",
);

const BARRIER_CREATE_CMD: CmdDesc = CmdDesc::new(
    c"create",
    c"create [-n NAME] BARRIER COUNT",
    c"\
Create a barrier for COUNT processes.

BARRIER receives an opaque integer handle (a bash variable). Without -n the barrier
is created in anonymous shared memory and is shared across forked processes
(for example a background job started with &). With -n NAME it is backed by a
named shared-memory object (shm_open) that unrelated processes can later open.

Examples:
  L_builtin barrier create var 2
  L_builtin barrier create -n /my_barrier v 3
",
);

const BARRIER_OPEN_CMD: CmdDesc = CmdDesc::new(
    c"open",
    c"open BARRIER NAME",
    c"\
Open an existing named barrier NAME and assign its handle to BARRIER.

The named barrier must already exist (created by another process with
'create -n NAME').

Examples:
  L_builtin barrier open w /my_barrier
",
);

const BARRIER_WAIT_CMD: CmdDesc = CmdDesc::new(
    c"wait",
    c"wait [-t SECS] [-n] BARRIER",
    c"\
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
",
);

const BARRIER_CLOSE_CMD: CmdDesc = CmdDesc::new(
    c"close",
    c"close BARRIER",
    c"\
Unmap the barrier BARRIER in the current process without destroying the shared
resource. Other processes keep their mappings.

Examples:
  L_builtin barrier close $var
",
);

const BARRIER_RESET_CMD: CmdDesc = CmdDesc::new(
    c"reset",
    c"reset BARRIER",
    c"\
Reset the barrier BARRIER for reuse: clears the satisfied state and the arrival
count so a fresh round can begin.

Examples:
  L_builtin barrier reset $var
",
);

const BARRIER_DESTROY_CMD: CmdDesc = CmdDesc::new(
    c"destroy",
    c"destroy BARRIER",
    c"\
Destroy the barrier BARRIER: unmap it in the current process and, for a named
barrier, unlink its shared-memory object globally.

Examples:
  L_builtin barrier destroy $var
",
);

const BARRIER_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("create", barrier_create_subcommand),
    ("open", barrier_open_subcommand),
    ("wait", barrier_wait_subcommand),
    ("close", barrier_close_subcommand),
    ("reset", barrier_reset_subcommand),
    ("destroy", barrier_destroy_subcommand),
];

const BARRIER_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 6> =
    crate::intlookup!(&BARRIER_SUBCOMMANDS);

#[derive(CmdArgs)]
struct BarrierDispatchArgs {
    #[positional]
    action: *const c_char,
    #[rest]
    rest: WordListIterCpnt<'static>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn barrier_subcommand(list: *mut WORD_LIST) -> c_int {
    BARRIER_CMD.enter();
    let args = match BarrierDispatchArgs::parse(list) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let action_bytes = unsafe { std::ffi::CStr::from_ptr(args.action) }.to_bytes();
    let handler = match BARRIER_TABLE.lookup(action_bytes) {
        Some(h) => h,
        None => {
            l_builtin_error!(b"unknown barrier subcommand: ", action_bytes);
            return EX_USAGE;
        }
    };
    handler(args.rest.as_ptr())
}
