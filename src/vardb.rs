//! `vardb` - shared-memory variable database backed by `rkyv` serialization.
//!
//! The database for one SHM region is a single `rkyv`-serialized blob. Indexed
//! arrays are stored as an index-to-value map; associative arrays as a
//! key-to-value map. Every element value carries its trailing NUL; the load
//! path hands bash a raw pointer it copies without extra allocation.
//!
//! ## Backends
//!
//! `| backing  | flag | layout                    | locking                  |`
//! `|----------|------|---------------------------|--------------------------|`
//! `| memfd    | -n   | `[rwlock][blob]`          | pshared pthread_rwlock_t |`
//! `| anon mmap| -M   | `[rwlock][len][blob]`     | pshared pthread_rwlock_t |`
//! `| POSIX shm| -s   | `[magic][blob]`           | flock + PID-tracked reopen |`
//! `| file     | -F   | `[magic][blob]`           | flock + PID-tracked reopen |`
//!
//! The `[-n]` memfd backend has no version header: it is created fresh in
//! process memory and shared only with forked children via the inherited fd +
//! mmap. A pshared `pthread_rwlock_t` in the mapped page provides exclusion.
//!
//! The `[-M]` anonymous-mmap backend is the same idea but a *fixed* size passed
//! by the user: a bounded `MAP_SHARED|MAP_ANONYMOUS` mapping with a pshared
//! rwlock, and the rkyv blob is length-prefixed so an empty region reads back as
//! an empty database. Because the region size is fixed, a write that would
//! exceed it fails (reported as an out-of-memory error) -- the bash variable is
//! already updated in that case, so the database may lag behind until the next
//! successful write (this is unavoidable for a bounded store).
//!
//! The `[-s]` shm and `[-F]` file backends persist by name/path and use `flock`.
//! Because `flock` is owned by the open-file-description, a fork-sibling that
//! inherited the fd shares the same OFD and gets *no* mutual exclusion. To fix
//! this, the backend stores the PID when the fd was opened. On every operation,
//! if the current PID differs, it re-opens the backing (by path/name) to obtain
//! a fresh open-file-description, so `flock` provides real cross-process
//! exclusion.
//!
//! ## Version handling
//!
//! A file/shm backing whose 32-byte magic header does not match `MAGIC` (i.e. an
//! incompatible/foreign store) is *not* read. The opener warns and
//! reinitializes it (stamps `MAGIC`, truncates the stale tail) so the next
//! operation starts from an empty `DatabaseRepr` instead of deserializing
//! incompatible bytes. An empty/short backing is stamped silently (first use).

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileExt;
use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
use std::path::PathBuf;

use rkyv::{Archive, Deserialize, Serialize};

use crate::beprintln;
use crate::shared::ensure_high_fd;

///////////////////////////////////////////////////////////////////////////
// Layout constants

/// 32-byte magic header: "L_builtin shm v1" followed by NUL padding.
const MAGIC: [u8; MAGIC_SIZE] = {
    let mut m = [0u8; MAGIC_SIZE];
    let magic = b"L_builtin shm v1";
    let mut i = 0;
    while i < magic.len() {
        m[i] = magic[i];
        i += 1;
    }
    m
};
/// Total size of the magic header.
const MAGIC_SIZE: usize = 32;
/// Offset at which the rkyv blob begins (immediately after the header).
const BLOB_OFFSET_FLOCK: u64 = MAGIC_SIZE as u64;

/// Compile-time check that the rwlock fits in a page on every platform.
const _RWLOCK_FITS_IN_PAGE: () = assert!(
    std::mem::size_of::<libc::pthread_rwlock_t>() < 4096,
    "pthread_rwlock_t must fit in one page"
);

///////////////////////////////////////////////////////////////////////////
// Data types

/// Map any rkyv/rancor error into an `io::Error` so the rest of the crate can
/// use a single error type.
fn rkyv_err<E: std::fmt::Display>(e: E) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, format!("rkyv: {}", e))
}

/// An in-memory representation of one bash variable's shared contents.
///
/// Indexed arrays are keyed by their integer index; associative arrays are
/// keyed by their string key. The values (and associative keys) are stored as
/// `CString`s that include their trailing NUL, so loading can hand bash a raw
/// pointer it copies without extra allocation.
#[derive(Archive, Serialize, Deserialize, Debug, Clone)]
pub enum VarData {
    Array(HashMap<i64, CString>),
    Assoc(HashMap<CString, CString>),
}

impl Default for VarData {
    fn default() -> Self {
        VarData::Array(HashMap::new())
    }
}

impl VarData {
    /// Insert one indexed-array element. Turns an empty/associative entry into
    /// an indexed array first. The value is stored with its trailing NUL.
    pub fn insert_index(&mut self, idx: i64, value: CString) {
        if !matches!(self, VarData::Array(_)) {
            *self = VarData::Array(HashMap::new());
        }
        if let VarData::Array(m) = self {
            m.insert(idx, value);
        }
    }

    /// Insert one associative-array element. Turns an empty/indexed entry into
    /// an associative array first. Both the key and value are stored with
    /// their trailing NUL.
    pub fn insert_key(&mut self, key: CString, value: CString) {
        if !matches!(self, VarData::Assoc(_)) {
            *self = VarData::Assoc(HashMap::new());
        }
        if let VarData::Assoc(m) = self {
            m.insert(key, value);
        }
    }

    pub fn as_array(&self) -> Option<&HashMap<i64, CString>> {
        match self {
            VarData::Array(m) => Some(m),
            _ => None,
        }
    }

    pub fn as_assoc(&self) -> Option<&HashMap<CString, CString>> {
        match self {
            VarData::Assoc(m) => Some(m),
            _ => None,
        }
    }
}

/// The full serialized database for one SHM region: every bound variable.
#[derive(Archive, Serialize, Deserialize, Debug, Default, Clone)]
pub struct DatabaseRepr {
    pub vars: HashMap<CString, VarData>,
}

///////////////////////////////////////////////////////////////////////////
// Backing descriptor

/// How a database is backed. A single enum is used for both selecting a store
/// (the `shm` backing flags) and as the runtime identity of an opened store
/// (`LockedDatabase::path()`); `DbPath` is the only locator type. `Mmap.size`
/// is `Some` when the store is created (`-M NAME:SIZE`) and `None` when only
/// selecting an existing named store (`-M NAME`).
#[derive(Clone, Debug)]
pub enum DbPath {
    /// A regular file at an arbitrary path (`-F PATH`).
    File(PathBuf),
    /// A POSIX shared memory object via `shm_open`, named by the user's
    /// `SHM_NAME` (`-s NAME`).
    Shm(CString),
    /// An in-memory file created with `memfd_create`, named with the `SHM_NAME`
    /// (the default; shared only with forked children).
    Mem(CString),
    /// A fixed-size anonymous `mmap(MAP_ANONYMOUS)`, named (`-M NAME`). `size`
    /// is `Some` at creation (`-M NAME:SIZE`); selection (`-M NAME`) carries
    /// `None`. Identified by name across commands.
    Mmap { name: CString, size: Option<u64> },
}

///////////////////////////////////////////////////////////////////////////
// Locking

/// Apply `flock(2)` with retrying on `EINTR`.
fn flock(fd: RawFd, op: libc::c_int) -> io::Result<()> {
    loop {
        let r = unsafe { libc::flock(fd, op) };
        if r == 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        if e.raw_os_error() == Some(libc::EINTR) {
            continue;
        }
        return Err(e);
    }
}

/// RAII guard that acquires a `flock(2)` lock on construction and releases it
/// (`LOCK_UN`) on drop.
struct FlockLock(RawFd);

impl FlockLock {
    fn new(fd: RawFd, op: libc::c_int) -> io::Result<Self> {
        flock(fd, op)?;
        Ok(FlockLock(fd))
    }
}

impl Drop for FlockLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.0, libc::LOCK_UN);
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// pshared rwlock helpers (for the memfd and anonymous-mmap backings)

unsafe fn init_pshared_rwlock(rwlock: *mut libc::pthread_rwlock_t) -> io::Result<()> {
    let mut attr: libc::pthread_rwlockattr_t = std::mem::zeroed();
    if libc::pthread_rwlockattr_init(&mut attr) != 0 {
        return Err(io::Error::last_os_error());
    }
    if libc::pthread_rwlockattr_setpshared(&mut attr, libc::PTHREAD_PROCESS_SHARED) != 0 {
        libc::pthread_rwlockattr_destroy(&mut attr);
        return Err(io::Error::last_os_error());
    }
    let rc = libc::pthread_rwlock_init(rwlock, &attr);
    libc::pthread_rwlockattr_destroy(&mut attr);
    if rc != 0 {
        return Err(io::Error::from_raw_os_error(rc));
    }
    Ok(())
}

/// RAII guard holding a read lock on a pshared `pthread_rwlock_t`.
struct RwLockReadGuard {
    rwlock: *mut libc::pthread_rwlock_t,
}

impl RwLockReadGuard {
    fn acquire(rwlock: *mut libc::pthread_rwlock_t) -> io::Result<Self> {
        let rc = unsafe { libc::pthread_rwlock_rdlock(rwlock) };
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        Ok(Self { rwlock })
    }
}

impl Drop for RwLockReadGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_unlock(self.rwlock);
        }
    }
}

/// RAII guard holding a write lock on a pshared `pthread_rwlock_t`.
struct RwLockWriteGuard {
    rwlock: *mut libc::pthread_rwlock_t,
}

impl RwLockWriteGuard {
    fn acquire(rwlock: *mut libc::pthread_rwlock_t) -> io::Result<Self> {
        let rc = unsafe { libc::pthread_rwlock_wrlock(rwlock) };
        if rc != 0 {
            return Err(io::Error::from_raw_os_error(rc));
        }
        Ok(Self { rwlock })
    }
}

impl Drop for RwLockWriteGuard {
    fn drop(&mut self) {
        unsafe {
            libc::pthread_rwlock_unlock(self.rwlock);
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Magic header / version check (file and POSIX-shm backings)

/// Outcome of examining a backing's magic header.
enum MagicStatus {
    /// Header matches `MAGIC`; existing blob is valid.
    Ok,
    /// Backing is empty/short: first creation; stamp `MAGIC` + empty blob.
    Fresh,
    /// Non-empty header differs from `MAGIC`: warn and reinitialize (do NOT
    /// read the incompatible tail).
    Mismatch,
}

/// Read the first `MAGIC_SIZE` bytes and classify the backing's header.
fn classify_magic(file: &File) -> io::Result<MagicStatus> {
    let mut buf = [0u8; MAGIC_SIZE];
    match file.read_exact_at(&mut buf, 0) {
        Ok(()) => {
            if buf == MAGIC {
                Ok(MagicStatus::Ok)
            } else {
                Ok(MagicStatus::Mismatch)
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::UnexpectedEof => Ok(MagicStatus::Fresh),
        Err(e) => Err(e),
    }
}

/// Stamp the magic header and truncate any stale blob tail, leaving an empty
/// (MAGIC-only) backing.
fn init_magic_and_truncate(file: &File) -> io::Result<()> {
    file.write_all_at(&MAGIC, 0)?;
    file.set_len(BLOB_OFFSET_FLOCK)?;
    file.sync_all()?;
    Ok(())
}

/// Inspect the header and, for `Fresh`/`Mismatch`, stamp it (warning + reinit
/// on mismatch so incompatible data is never read).
fn init_or_check_magic(file: &File, shm_name: &OsStr) -> io::Result<()> {
    match classify_magic(file)? {
        MagicStatus::Ok => Ok(()),
        MagicStatus::Fresh => init_magic_and_truncate(file),
        MagicStatus::Mismatch => {
            beprintln!(
                b"L_builtin: shm: ",
                shm_name,
                b": header mismatch, reinitializing"
            );
            init_magic_and_truncate(file)
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Blob read/write

/// Read the rkyv blob at `offset` from `file`. Returns `None` if the file is
/// too short to contain any data at that offset.
fn read_blob_at(file: &File, offset: u64) -> io::Result<Option<Vec<u8>>> {
    let len = file.metadata()?.len();
    if len <= offset {
        return Ok(None);
    }
    let blob_len = (len - offset) as usize;
    let mut buf = vec![0u8; blob_len];
    file.read_exact_at(&mut buf, offset)?;
    if buf.len() < std::mem::size_of::<u64>() {
        return Ok(None);
    }
    Ok(Some(buf))
}

/// Read and deserialize the rkyv blob at `offset` from `file`, returning a fresh
/// `DatabaseRepr` (or default if the file is empty / has no blob yet).
fn read_blob_into(file: &File, offset: u64) -> io::Result<DatabaseRepr> {
    use rkyv::util::AlignedVec;
    match read_blob_at(file, offset)? {
        None => Ok(DatabaseRepr::default()),
        Some(bytes) => {
            let mut aligned = AlignedVec::<16>::new();
            aligned.extend_from_slice(&bytes);
            rkyv::from_bytes::<DatabaseRepr, rkyv::rancor::Error>(&aligned).map_err(rkyv_err)
        }
    }
}

/// Serialize `repr` and write it at `offset` in `file`, truncating to the exact
/// blob size (so a smaller blob does not leave a stale tail).
fn write_blob_at(file: &File, repr: &DatabaseRepr, offset: u64) -> io::Result<()> {
    use rkyv::util::AlignedVec;
    let bytes: AlignedVec = rkyv::to_bytes::<rkyv::rancor::Error>(repr).map_err(rkyv_err)?;
    file.set_len(offset + bytes.len() as u64)?;
    file.write_all_at(&bytes, offset)?;
    file.sync_all()?;
    Ok(())
}

/// Read and deserialize a length-prefixed rkyv blob from an in-memory region.
/// The region layout is `[u64 len][blob bytes ...]` starting at `len_offset`.
/// A zero/oversize/missing length reads back as an empty database (never
/// deserializes garbage).
fn read_blob_from_mem(mem: &[u8], len_offset: usize) -> io::Result<DatabaseRepr> {
    use rkyv::util::AlignedVec;
    let len_size = std::mem::size_of::<u64>();
    let data_off = len_offset + len_size;
    let avail = mem.len().checked_sub(data_off);
    if avail.is_none() {
        return Ok(DatabaseRepr::default());
    }
    let blob_len =
        u64::from_ne_bytes(mem[len_offset..len_offset + len_size].try_into().unwrap()) as usize;
    if blob_len == 0 || avail.is_none_or(|a| a < blob_len) {
        return Ok(DatabaseRepr::default());
    }
    let blob = &mem[data_off..data_off + blob_len];
    if blob.len() < std::mem::size_of::<u64>() {
        return Ok(DatabaseRepr::default());
    }
    let mut aligned = AlignedVec::<16>::new();
    aligned.extend_from_slice(blob);
    rkyv::from_bytes::<DatabaseRepr, rkyv::rancor::Error>(&aligned).map_err(rkyv_err)
}

/// Serialize `repr` and store it length-prefixed at `len_offset` in `mem`. Fails
/// with `OutOfMemory` if the fixed region cannot hold the blob -- the caller
/// (the rkyv blob grows on demand).
fn write_blob_to_mem(mem: &mut [u8], repr: &DatabaseRepr, len_offset: usize) -> io::Result<()> {
    use rkyv::util::AlignedVec;
    let bytes: AlignedVec = rkyv::to_bytes::<rkyv::rancor::Error>(repr).map_err(rkyv_err)?;
    let len_size = std::mem::size_of::<u64>();
    let data_off = len_offset + len_size;
    let end = data_off
        .checked_add(bytes.len())
        .ok_or_else(|| io::Error::new(io::ErrorKind::OutOfMemory, "shm: -M blob size overflow"))?;
    if end > mem.len() {
        return Err(io::Error::new(
            io::ErrorKind::OutOfMemory,
            "shm: -M mmap region exhausted",
        ));
    }
    mem[len_offset..len_offset + len_size].copy_from_slice(&(bytes.len() as u64).to_ne_bytes());
    mem[data_off..end].copy_from_slice(&bytes);
    Ok(())
}

///////////////////////////////////////////////////////////////////////////
// Open-file-description state (shared by the file and POSIX-shm backings)

/// Open-file-description state shared by the file (`-F`) and POSIX-shm (`-s`)
/// backings: the open `File` plus the PID captured when it was opened. The PID
/// is recorded so a forked child can detect that its inherited fd shares an
/// open-file-description and reopen for real cross-process `flock` exclusion.
struct FlockState {
    file: File,
    pid: u32,
}

impl FlockState {
    /// Re-open via `reopen` if we forked since `file` was opened (PID changed).
    /// `reopen` returns a freshly-opened `File` whose magic header is already
    /// (re)validated, so callers only need to update the PID.
    fn refresh(&mut self, reopen: impl FnOnce() -> io::Result<File>) -> io::Result<()> {
        if self.pid != std::process::id() {
            self.file = reopen()?;
            self.pid = std::process::id();
        }
        Ok(())
    }

    fn as_raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

/// Open (creating if necessary) the backing fd for a flock-backed store
/// (`-F`/`-s`), without touching the header.
fn open_flock_fd(path: &DbPath) -> io::Result<File> {
    match path {
        DbPath::File(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(p)?;
            let fd = file.into_raw_fd();
            unsafe { Ok(File::from_raw_fd(ensure_high_fd(fd, true)?)) }
        }
        DbPath::Shm(n) => shm_open_obj(n),
        _ => unreachable!("flock backing is only -s/-F; got {path:?}"),
    }
}

/// Open the backing fd and stamp/reinitialize its magic header.
fn open_flock_fd_checked(path: &DbPath) -> io::Result<File> {
    let file = open_flock_fd(path)?;
    init_or_check_magic(&file, db_label(path))?;
    Ok(file)
}

/// Human-readable label for a backing, used in warnings.
fn db_label(path: &DbPath) -> &OsStr {
    match path {
        DbPath::File(p) => p.file_name().unwrap_or_else(|| p.as_os_str()),
        DbPath::Shm(n) | DbPath::Mem(n) => OsStr::from_bytes(n.to_bytes()),
        DbPath::Mmap { name, .. } => OsStr::from_bytes(name.to_bytes()),
    }
}

///////////////////////////////////////////////////////////////////////////
// memfd / anonymous-mmap plumbing

/// Build the `shm_open` object name for a user `SHM_NAME`: `shm_open` requires
/// the name to start with `/`, so we prepend one.
fn shm_object_name(name: &CStr) -> CString {
    CString::new(["/".as_bytes(), name.to_bytes()].concat()).unwrap_or_default()
}

/// Create an anonymous in-memory file via `memfd_create` (the libc-provided
/// function, not a raw syscall) and return it as a high fd.
fn memfd_create_fd(name: &CStr) -> io::Result<File> {
    let fd =
        unsafe { libc::memfd_create(name.as_ptr(), libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let new_fd = ensure_high_fd(fd as RawFd, true)?;
    unsafe { Ok(File::from_raw_fd(new_fd)) }
}

/// Open (creating if necessary) a POSIX shared memory object via `shm_open`.
fn shm_open_obj(name: &CStr) -> io::Result<File> {
    let fd = unsafe {
        libc::shm_open(
            shm_object_name(name).as_ptr(),
            libc::O_CREAT | libc::O_RDWR,
            0o600,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    let new_fd = ensure_high_fd(fd as RawFd, true)?;
    unsafe { Ok(File::from_raw_fd(new_fd)) }
}

/// Remove a POSIX shared memory object via `shm_unlink`. Errors are ignored
/// (e.g. already removed); the only purpose is cleanup.
fn shm_unlink_obj(name: &CStr) {
    let obj = shm_object_name(name);
    unsafe {
        libc::shm_unlink(obj.as_ptr());
    }
}

/// Create a fixed-size `MAP_SHARED|MAP_ANONYMOUS` mapping (inherited across
/// fork). Fails with `ENOMEM` (-> `std::io::ErrorKind::OutOfMemory`) if the
/// kernel cannot satisfy the allocation.
fn anon_mmap(size: usize) -> io::Result<*mut u8> {
    if size == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "shm: -M size must be greater than 0",
        ));
    }
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_SHARED | libc::MAP_ANONYMOUS,
            -1 as RawFd,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return Err(io::Error::last_os_error());
    }
    Ok(ptr as *mut u8)
}

///////////////////////////////////////////////////////////////////////////
// Backends

/// The single shared flock-based backend for the file (`-F`) and POSIX-shm
/// (`-s`) backings. Both differ only in how they (re)open their backing; the
/// flock/lock/read/write dance is identical, so it lives here once. The
/// `path` is the [`DbPath`] locator (also used to reopen and as the store's
/// identity); no separate `FlockKind` enum is needed.
struct FlockBackend {
    path: DbPath,
    state: RefCell<FlockState>,
}

impl FlockBackend {
    fn open(path: DbPath) -> io::Result<Self> {
        let file = open_flock_fd_checked(&path)?;
        Ok(Self {
            path,
            state: RefCell::new(FlockState {
                file,
                pid: std::process::id(),
            }),
        })
    }

    fn refresh(&self) -> io::Result<()> {
        let path = self.path.clone();
        self.state
            .borrow_mut()
            .refresh(|| open_flock_fd_checked(&path))
    }

    fn shm_name(&self) -> &OsStr {
        db_label(&self.path)
    }

    fn db_path(&self) -> DbPath {
        self.path.clone()
    }

    fn read(&self) -> io::Result<DatabaseRepr> {
        self.refresh()?;
        let st = self.state.borrow();
        let _rel = FlockLock::new(st.as_raw_fd(), libc::LOCK_SH)?;
        read_blob_into(&st.file, BLOB_OFFSET_FLOCK)
    }

    fn with_write<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut DatabaseRepr) -> R,
    {
        self.refresh()?;
        let st = self.state.borrow();
        let _rel = FlockLock::new(st.as_raw_fd(), libc::LOCK_EX)?;
        let mut repr = read_blob_into(&st.file, BLOB_OFFSET_FLOCK)?;
        let r = f(&mut repr);
        write_blob_at(&st.file, &repr, BLOB_OFFSET_FLOCK)?;
        Ok(r)
    }
}

/// `memfd_create`-backed database: a pshared `pthread_rwlock_t` in a `MAP_SHARED`
/// mapping, immediately followed by the rkyv blob. No version header (the
/// memfd is ephemeral, created fresh in process memory); the file length
/// indicates whether a blob exists yet.
struct MemfdBackend {
    file: File,
    name: CString,
    rwlock: *mut libc::pthread_rwlock_t,
}

impl MemfdBackend {
    fn open(name: &CStr) -> io::Result<Self> {
        let file = memfd_create_fd(name)?;
        let rwlock_size = std::mem::size_of::<libc::pthread_rwlock_t>();
        let len = file.metadata()?.len();
        if len < rwlock_size as u64 {
            file.set_len(rwlock_size as u64)?;
        }
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                rwlock_size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        if ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }
        let rwlock = ptr as *mut libc::pthread_rwlock_t;
        unsafe {
            init_pshared_rwlock(rwlock)?;
        }
        Ok(Self {
            file,
            name: name.to_owned(),
            rwlock,
        })
    }

    fn blob_offset(&self) -> u64 {
        std::mem::size_of::<libc::pthread_rwlock_t>() as u64
    }

    fn shm_name(&self) -> &OsStr {
        OsStr::from_bytes(self.name.to_bytes())
    }

    fn db_path(&self) -> DbPath {
        DbPath::Mem(self.name.clone())
    }

    fn read(&self) -> io::Result<DatabaseRepr> {
        let _guard = RwLockReadGuard::acquire(self.rwlock)?;
        read_blob_into(&self.file, self.blob_offset())
    }

    fn with_write<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut DatabaseRepr) -> R,
    {
        let _guard = RwLockWriteGuard::acquire(self.rwlock)?;
        let mut repr = read_blob_into(&self.file, self.blob_offset())?;
        let r = f(&mut repr);
        write_blob_at(&self.file, &repr, self.blob_offset())?;
        Ok(r)
    }
}

impl Drop for MemfdBackend {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(
                self.rwlock as *mut libc::c_void,
                std::mem::size_of::<libc::pthread_rwlock_t>(),
            );
        }
    }
}

/// Fixed-size anonymous `MAP_SHARED|MAP_ANONYMOUS` backing (`-M SIZE`): a
/// pshared `pthread_rwlock_t` at offset 0, a length-prefixed rkyv blob
/// right after, and the rest of the fixed region for growth. A write that no
/// longer fits fails with `OutOfMemory` (see [`write_blob_to_mem`]).
struct MmapBackend {
    ptr: *mut u8,
    size: usize,
    rwlock: *mut libc::pthread_rwlock_t,
    /// Length-prefix offset (immediately after the rwlock).
    len_offset: usize,
    /// User name (`-M NAME:SIZE`). Always present; the mapping is anonymous
    /// but the name is the store identity for registry lookups.
    name: CString,
}

impl MmapBackend {
    fn open(size: u64, name: CString) -> io::Result<Self> {
        let size = usize::try_from(size).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "shm: -M size out of range")
        })?;
        let rwlock_size = std::mem::size_of::<libc::pthread_rwlock_t>();
        if size < rwlock_size + std::mem::size_of::<u64>() + std::mem::size_of::<u64>() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "shm: -M size too small",
            ));
        }
        let ptr = anon_mmap(size)?;
        let rwlock = ptr as *mut libc::pthread_rwlock_t;
        unsafe {
            init_pshared_rwlock(rwlock)?;
        }
        Ok(Self {
            ptr,
            size,
            rwlock,
            len_offset: rwlock_size,
            name,
        })
    }

    fn shm_name(&self) -> &OsStr {
        OsStr::from_bytes(self.name.to_bytes())
    }

    fn db_path(&self) -> DbPath {
        DbPath::Mmap {
            name: self.name.clone(),
            size: None,
        }
    }

    fn read(&self) -> io::Result<DatabaseRepr> {
        let _guard = RwLockReadGuard::acquire(self.rwlock)?;
        let mem = unsafe { std::slice::from_raw_parts(self.ptr, self.size) };
        read_blob_from_mem(mem, self.len_offset)
    }

    fn with_write<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut DatabaseRepr) -> R,
    {
        let _guard = RwLockWriteGuard::acquire(self.rwlock)?;
        let mem = unsafe { std::slice::from_raw_parts_mut(self.ptr, self.size) };
        let mut repr = read_blob_from_mem(mem, self.len_offset)?;
        let r = f(&mut repr);
        write_blob_to_mem(mem, &repr, self.len_offset)?;
        Ok(r)
    }
}

impl Drop for MmapBackend {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut libc::c_void, self.size);
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Public handle

/// The flat dispatcher: `LockedDatabase` *is* the enum over the three concrete
/// backends (no separate `Backend` indirection). Each variant holds only the
/// data it needs.
#[allow(private_interfaces)]
pub enum LockedDatabase {
    /// `-s NAME` or `-F PATH`: flock-based, reopened per-PID for cross-process
    /// exclusion.
    Flock(FlockBackend),
    /// `-n NAME` / default: growable memfd, pshared rwlock.
    Memfd(MemfdBackend),
    /// `-M SIZE`: fixed-size anonymous mmap, pshared rwlock, length-prefixed blob.
    Mmap(MmapBackend),
}

impl LockedDatabase {
    pub fn open_file(path: PathBuf) -> io::Result<Self> {
        Ok(Self::Flock(FlockBackend::open(DbPath::File(path))?))
    }

    pub fn open_shm(shm: CString) -> io::Result<Self> {
        Ok(Self::Flock(FlockBackend::open(DbPath::Shm(shm))?))
    }

    pub fn open_mem(shm: &CStr) -> io::Result<Self> {
        Ok(Self::Memfd(MemfdBackend::open(shm)?))
    }

    /// Create a fixed-size anonymous `mmap` database of `size` bytes (`-M NAME:SIZE`).
    /// `name` is the store identity (always required; the mapping is anonymous).
    pub fn open_mmap(size: u64, name: CString) -> io::Result<Self> {
        Ok(Self::Mmap(MmapBackend::open(size, name)?))
    }

    /// Remove the POSIX shared memory object backing `shm` (the user's
    /// `SHM_NAME`). Safe to call even if it was already removed.
    pub fn unlink_shm(shm: &CStr) {
        shm_unlink_obj(shm);
    }

    /// The SHM_NAME this database belongs to: the file name for `File`, or the
    /// name for `Shm`/`Mem`/`Mmap`.
    pub fn shm_name(&self) -> &OsStr {
        match self {
            LockedDatabase::Flock(b) => b.shm_name(),
            LockedDatabase::Memfd(b) => b.shm_name(),
            LockedDatabase::Mmap(b) => b.shm_name(),
        }
    }

    /// The path identifying this database's backing (owned copy).
    pub fn path(&self) -> DbPath {
        match self {
            LockedDatabase::Flock(b) => b.db_path(),
            LockedDatabase::Memfd(b) => b.db_path(),
            LockedDatabase::Mmap(b) => b.db_path(),
        }
    }

    /// Reload the whole database (shared lock) and return it.
    pub fn read(&self) -> io::Result<DatabaseRepr> {
        match self {
            LockedDatabase::Flock(b) => b.read(),
            LockedDatabase::Memfd(b) => b.read(),
            LockedDatabase::Mmap(b) => b.read(),
        }
    }

    pub fn with_write<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut DatabaseRepr) -> R,
    {
        match self {
            LockedDatabase::Flock(b) => b.with_write(f),
            LockedDatabase::Memfd(b) => b.with_write(f),
            LockedDatabase::Mmap(b) => b.with_write(f),
        }
    }
}

///////////////////////////////////////////////////////////////////////////
// Open entry point

/// Open a fresh [`LockedDatabase`] for the given locator. `DbPath` is the only
/// locator type; `locked_db_open` is the creation path used by the `add`/`bind`
/// command when a store is not yet in the registry.
pub fn open_db_loc(loc: &DbPath) -> Result<LockedDatabase, String> {
    match loc {
        DbPath::File(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("shm: cannot create {}: {}", parent.display(), e))?;
            }
            LockedDatabase::open_file(p.clone())
                .map_err(|e| format!("shm: cannot open {}: {}", p.display(), e))
        }
        DbPath::Shm(name) => LockedDatabase::open_shm(name.clone()).map_err(|e| {
            format!(
                "shm: cannot open shared memory {}: {}",
                name.to_str().unwrap_or(""),
                e
            )
        }),
        DbPath::Mem(name) => LockedDatabase::open_mem(name)
            .map_err(|e| format!("shm: cannot create anonymous database: {}", e)),
        DbPath::Mmap { name, size } => {
            let size = match size {
                Some(s) => *s,
                None => {
                    return Err("shm: -M NAME requires -M NAME:SIZE to create".into());
                }
            };
            LockedDatabase::open_mmap(size, name.clone())
                .map_err(|e| format!("shm: cannot create anonymous mmap database: {}", e))
        }
    }
}
