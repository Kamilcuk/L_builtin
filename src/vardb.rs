//! `vardb` - shared-memory variable database backed by `rkyv` serialization.
//!
//! The whole database for one SHM region is a single `rkyv`-serialized blob
//! stored in a file under `/dev/shm` (see [`crate::cmd_shm`]). The blob holds a
//! map from bash variable name to that variable's contents:
//!   * indexed arrays   -> `VarData::Array(HashMap<i64, String>)`
//!   * associative arrays -> `VarData::Assoc(HashMap<String, String>)`
//!
//! The element *values* are stored without their trailing NUL; the load path
//! re-adds the NUL when handing the string to bash's `array_insert` /
//! `l_assoc_insert`.
//!
//! A cross-process `flock` serializes readers and writers. Every read reloads
//! the blob from disk and every write reloads, modifies and rewrites the blob,
//! so a process always observes the latest shared state (unlike an LMDB/redb
//! style handle whose in-memory root can go stale across a fork).

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::fs::{File, OpenOptions};
use std::io;
use std::os::fd::RawFd;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::FileExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use rkyv::{util::AlignedVec, Archive, Deserialize, Serialize};

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

//////////////////////////////////////////////////////////////
// Cross-process flock

pub struct FileLock {
    file: File,
}

fn flock(file: &File, op: libc::c_int) -> io::Result<()> {
    loop {
        let r = unsafe { libc::flock(file.as_raw_fd(), op) };
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

impl FileLock {
    pub fn shared(file: File) -> io::Result<Self> {
        flock(&file, libc::LOCK_SH)?;
        Ok(Self { file })
    }

    pub fn exclusive(file: File) -> io::Result<Self> {
        flock(&file, libc::LOCK_EX)?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        unsafe {
            libc::flock(self.file.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

//////////////////////////////////////////////////////////////
// rkyv blob read/write

fn read_blob(file: &File) -> io::Result<Option<Vec<u8>>> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(None);
    }
    let mut buf = vec![0u8; len as usize];
    file.read_exact_at(&mut buf, 0)?;
    // The blob must be at least the size of an archived root pointer record.
    if buf.len() < std::mem::size_of::<u64>() {
        return Ok(None);
    }
    Ok(Some(buf))
}

fn write_blob(file: &File, repr: &DatabaseRepr) -> io::Result<()> {
    let bytes: AlignedVec = rkyv::to_bytes::<rkyv::rancor::Error>(repr).map_err(rkyv_err)?;
    // Shrink first so a smaller blob does not leave a stale tail behind.
    file.set_len(bytes.len() as u64)?;
    file.write_all_at(&bytes, 0)?;
    file.sync_all()?;
    Ok(())
}

//////////////////////////////////////////////////////////////
// Process-locked database

/// A shared-memory variable database backed by a `rkyv` blob on disk.
///
/// A single open file (`file`) backs both the cross-process `flock` and the
/// serialized blob (read/written at offset 0). The file descriptor is kept
/// open for the lifetime of the database. `read` and `with_write` always
/// reload the blob from disk under the flock, so the latest shared state is
/// always observed. The in-memory `database` is kept in sync for cheap
/// in-process reuse.
/// How a database is backed.
pub enum DbPath {
    /// A regular file at an arbitrary path (`-p PATH`, "path on a disc").
    File(PathBuf),
    /// A POSIX shared memory object via `shm_open`/`shm_unlink`, named by the
    /// user's `SHM_NAME` (`-s NAME`). This is name-based shared memory, not a
    /// hardcoded `/dev/shm` file.
    Shm(CString),
    /// An in-memory file created with `memfd_create`, named with the `SHM_NAME`
    /// (the default; shared only with forked children).
    Mem(CString),
}

/// How the positional identifier maps to a backing store.
pub enum DbLoc {
    /// `-f PATH`: a regular file at an arbitrary path ("path on a disc").
    File(PathBuf),
    /// `-s NAME`: a POSIX shared memory object opened with `shm_open`.
    Shm(CString),
    /// Neither flag: an in-memory `memfd_create` named with the identifier.
    Mem(CString),
}

/// Open a fresh [`LockedDatabase`] for the given location.
pub fn open_db_loc(loc: &DbLoc) -> Result<LockedDatabase, String> {
    match loc {
        DbLoc::File(p) => {
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("shm: cannot create {}: {}", parent.display(), e))?;
            }
            LockedDatabase::open_file(p).map_err(|e| format!("shm: cannot open {}: {}", p.display(), e))
        }
        DbLoc::Shm(name) => LockedDatabase::open_shm(name).map_err(|e| {
            format!(
                "shm: cannot open shared memory {}: {}",
                name.to_str().unwrap_or(""),
                e
            )
        }),
        DbLoc::Mem(name) => LockedDatabase::open_mem(name)
            .map_err(|e| format!("shm: cannot create anonymous database: {}", e)),
    }
}

pub struct LockedDatabase {
    /// Backing of this database.
    pub path: DbPath,
    /// Open file backing the database: holds the `flock` and the rkyv blob.
    /// For `Mem`, this is the `memfd_create` fd.
    pub file: File,
    database: Mutex<DatabaseRepr>,
}

/// Create an anonymous in-memory file via `memfd_create` and return it as a
/// `File`. The returned fd is close-on-exec and allows sealing.
fn memfd_create(name: &CStr) -> io::Result<File> {
    let fd = unsafe {
        libc::syscall(
            libc::SYS_memfd_create,
            name.as_ptr(),
            libc::MFD_CLOEXEC | libc::MFD_ALLOW_SEALING,
        )
    };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd as RawFd) })
}

/// Build the `shm_open` object name for a user `SHM_NAME`: `shm_open` requires
/// the name to start with `/`, so we prepend one.
fn shm_object_name(name: &CStr) -> CString {
    CString::new(["/".as_bytes(), name.to_bytes()].concat()).unwrap_or_default()
}

/// Open (creating if necessary) a POSIX shared memory object via `shm_open`.
fn shm_open_obj(name: &CStr) -> io::Result<File> {
    let obj = shm_object_name(name);
    let fd = unsafe { libc::shm_open(obj.as_ptr(), libc::O_CREAT | libc::O_RDWR, 0o600) };
    if fd < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_fd(fd as RawFd) })
}

/// Remove a POSIX shared memory object via `shm_unlink`. Errors are ignored
/// (e.g. already removed); the only purpose is cleanup.
fn shm_unlink_obj(name: &CStr) {
    let obj = shm_object_name(name);
    unsafe {
        libc::shm_unlink(obj.as_ptr());
    }
}

impl LockedDatabase {
    /// Open (creating if necessary) the named database file at `db_path`. The
    /// same file backs both the cross-process `flock` and the serialized blob,
    /// and is kept open for the lifetime of the returned handle.
    pub fn open_file(db_path: &Path) -> io::Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(db_path)?;
        Ok(Self {
            path: DbPath::File(db_path.to_owned()),
            file,
            database: Mutex::new(DatabaseRepr::default()),
        })
    }

    /// Create an in-memory database backed by a `memfd_create` fd named
    /// `shm`. The name is only used for diagnostic/`/proc` display; the database
    /// is shared with forked children via the fd.
    pub fn open_mem(shm: &CStr) -> io::Result<Self> {
        let file = memfd_create(shm)?;
        Ok(Self {
            path: DbPath::Mem(shm.to_owned()),
            file,
            database: Mutex::new(DatabaseRepr::default()),
        })
    }

    /// Open (creating if necessary) a POSIX shared memory database via
    /// `shm_open`, named `shm` (the user's `SHM_NAME`, prefixed with `/`).
    pub fn open_shm(shm: &CStr) -> io::Result<Self> {
        let file = shm_open_obj(shm)?;
        Ok(Self {
            path: DbPath::Shm(shm.to_owned()),
            file,
            database: Mutex::new(DatabaseRepr::default()),
        })
    }

    /// Remove the POSIX shared memory object backing `shm` (the user's
    /// `SHM_NAME`). Safe to call even if it was already removed.
    pub fn unlink_shm(shm: &CStr) {
        shm_unlink_obj(shm);
    }

    /// The SHM_NAME this database belongs to: the file name for `File`, or the
    /// name for `Shm`/`Mem`.
    pub fn shm_name(&self) -> &OsStr {
        match &self.path {
            DbPath::File(p) => p.file_name().unwrap_or_else(|| p.as_os_str()),
            DbPath::Shm(n) => OsStr::from_bytes(n.to_bytes()),
            DbPath::Mem(n) => OsStr::from_bytes(n.to_bytes()),
        }
    }

    fn load(&self) -> io::Result<DatabaseRepr> {
        match read_blob(&self.file)? {
            None => Ok(DatabaseRepr::default()),
            Some(bytes) => {
                let mut aligned = AlignedVec::<16>::new();
                aligned.extend_from_slice(&bytes);
                let repr = rkyv::from_bytes::<DatabaseRepr, rkyv::rancor::Error>(&aligned)
                    .map_err(rkyv_err)?;
                Ok(repr)
            }
        }
    }

    /// Reload the whole database from disk (shared lock) and return it.
    pub fn read(&self) -> io::Result<DatabaseRepr> {
        let _lock = FileLock::shared(self.file.try_clone()?)?;
        let repr = self.load()?;
        *self.database.lock().unwrap() = repr.clone();
        Ok(repr)
    }

    /// Reload the database (exclusive lock), apply `f` to mutate it, then write
    /// the result back to disk. Returns whatever `f` returned.
    pub fn with_write<F, R>(&self, f: F) -> io::Result<R>
    where
        F: FnOnce(&mut DatabaseRepr) -> R,
    {
        let _lock = FileLock::exclusive(self.file.try_clone()?)?;
        let mut repr = self.load()?;
        let r = f(&mut repr);
        write_blob(&self.file, &repr)?;
        *self.database.lock().unwrap() = repr;
        Ok(r)
    }
}
