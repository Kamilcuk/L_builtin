use std::io;
use std::os::unix::io::RawFd;
use std::ptr;
use std::slice;
use redb::{Error as RedbError, StorageBackend};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::unix::io::{AsRawFd, FromRawFd, RawFd};
use std::bash_api::{ARRAY_ELEMENT, ARRAY}
use std::os::unix::fs::FileExt;


//////////////////////////////////////////////////////////////
// Bash ARRAY iterator

struct ArrayIterator {
    head: *mut ARRAY_ELEMENT,
    current: *mut ARRAY_ELEMENT,
}

impl ArrayIterator {
    unsafe fn new(arr: *mut ARRAY) -> Self {
        let head = l_array_head(arr);
        Self {
            head,
            current: if head.is_null() { std::ptr::null_mut() } else { l_element_forw(head) },
        }
    }
}

impl Iterator for ArrayIterator {
    type Item = (i64, &'static CStr);

    fn next(&mut self) -> Option<Self::Item> {
        if self.current == self.head {
            return None;
        }
        unsafe {
            let idx = l_element_index(self.current);
            let val = l_element_value(self.current);
            let cstr = if val.is_null() {
                CStr::from_bytes_with_nul_unchecked(b"\0")
            } else {
                CStr::from_ptr(val)
            };

            self.current = l_element_forw(self.current);
            Some((idx, cstr))
        }
    }
}

//////////////////////////////////////////////////////////////
/// Bash associative array iterator

struct AssocIterator {
    keys_list: *mut WORD_LIST,
    current: *mut WORD_LIST,
    hash: *mut HASH_TABLE,
}

impl AssocIterator {
    unsafe fn new(hash: *mut HASH_TABLE) -> Self {
        let keys = assoc_keys_to_word_list(hash);
        Self {
            keys_list: keys,
            current: keys,
            hash,
        }
    }
}

impl Iterator for AssocIterator {
    type Item = (&'static CStr, &'static CStr);

    fn next(&mut self) -> Option<Self::Item> {
        while !self.current.is_null() {
            unsafe {
                let wl = self.current;
                self.current = (*wl).next;
                let word_ptr = (*wl).word;
                if word_ptr.is_null() {
                    continue;
                }
                let key_ptr = (*word_ptr).word;
                if key_ptr.is_null() {
                    continue;
                }
                let key_cstr = CStr::from_ptr(key_ptr);
                let val_ptr = assoc_reference(self.hash, key_ptr);
                let val_cstr = if val_ptr.is_null() {
                    CStr::from_bytes_with_nul_unchecked(b"\0")
                } else {
                    CStr::from_ptr(val_ptr)
                };
                return Some((key_cstr, val_cstr));
            }
        }
        None
    }
}

impl Drop for AssocIterator {
    fn drop(&mut self) {
        if !self.keys_list.is_null() {
            unsafe {
                dispose_words(self.keys_list);
            }
        }
    }
}

//////////////////////////////////////////////////////////////
// File desctriptor backend for redb

pub struct FdBackend {
    file: File,
}

impl FdBackend {
    /// Create a backend from an existing raw file descriptor.
    ///
    /// # Safety
    /// The caller must ensure that the raw file descriptor is valid,
    /// open for reading and writing, and that ownership is safely transferred
    /// to this structure.
    pub unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self {
            file: File::from_raw_fd(fd),
        }
    }

    pub fn new(file: File) -> Self {
        Self { file }
    }
}

impl StorageBackend for FdBackend {
    fn len(&self) -> Result<u64, RedbError> {
        let metadata = self.file.metadata().map_err(RedbError::Io)?;
        Ok(metadata.len())
    }

    fn read(&self, offset: u64, out: &mut [u8]) -> Result<(), RedbError> {
        self.file
            .read_exact_at(out, offset)
            .map_err(RedbError::Io)
    }

    fn set_len(&self, len: u64) -> Result<(), RedbError> {
        self.file.set_len(len).map_err(RedbError::Io)
    }

    fn sync_data(&self) -> Result<(), RedbError> {
        self.file.sync_data().map_err(RedbError::Io)
    }

    fn write(&self, offset: u64, data: &[u8]) -> Result<(), RedbError> {
        self.file
            .write_all_at(data, offset)
            .map_err(RedbError::Io)
    }

    fn close(&self) -> Result<(), RedbError> {
        // redb calls close exactly once when the database is dropped.
        Ok(())
    }
}

impl AsRawFd for FdBackend {
    fn raw_fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }
}

//////////////////////////////////////////////////////////////
// Redb process locked database

pub struct ReadTxnGuard<'a> {
    _lock: FileLock,
    txn: redb::ReadTransaction<'a>,
}

pub struct WriteTxnGuard<'a> {
    _lock: FileLock,
    txn: redb::WriteTransaction<'a>,
}

impl WriteTxnGuard<'_> {
    pub fn open_array(&mut self, name: &str) -> Result<redb::Table<i64, &[u8]>, redb::Error> {
        self.txn.open_table(redb::TableDefinition::new(name))
    }
    pub fn open_assoc(&mut self, name: &str) -> Result<redb::Table<&[u8], &[u8]>, redb::Error> {
        self.txn.open_table(redb::TableDefinition::new(name))
    }
}

impl ReadTxnGuard<'_> {
    pub fn open_array(&mut self, name: &str) -> Result<redb::Table<i64, &[u8]>, redb::Error> {
        self.txn.open_table(redb::TableDefinition::new(name))
    }
    pub fn open_assoc(&mut self, name: &str) -> Result<redb::Table<&[u8], &[u8]>, redb::Error> {
        self.txn.open_table(redb::TableDefinition::new(name))
    }
}

pub struct LockedDatabase {
    db: Database,
    lock_file: File,
}

impl LockedDatabase {
    pub fn begin_read(&self) -> io::Result<ReadTxnGuard<'_>> {
        let lock = FileLock::shared(self.lock_file.try_clone()?)?;
        let txn = self.db.begin_read().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(ReadTxnGuard { _lock: lock, txn })
    }

    pub fn begin_write(&self) -> io::Result<WriteTxnGuard<'_>> {
        let lock = FileLock::exclusive(self.lock_file.try_clone()?)?;
        let txn = self.db.begin_write().map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;
        Ok(WriteTxnGuard { _lock: lock, txn })
    }
}

//////////////////////////////////////////////////////////////
// Write vairable into database

/// Trait to unify array and associative table operations and iteration.
trait BashTable<'db> {
    type Key: redb::Key + 'db;
    type Value: redb::Value + 'db;
    fn open(txn: &mut WriteTxnGuard<'_>, name: &str) -> Result<Self, redb::Error>
    where
        Self: Sized;
    fn table(&mut self) -> &mut redb::Table<Self::Key, Self::Value>;
}

struct ArrayTable<'db>(redb::Table<'db, u64, &'db [u8]>);
impl<'db> BashTable<'db> for ArrayTable<'db> {
    type Key = u64;
    type Value = &'db [u8];
    fn open(txn: &mut WriteTxnGuard<'_>, name: &str) -> Result<Self, redb::Error> {
        Ok(Self(txn.open_array(name)?))
    }
    fn table(&mut self) -> &mut redb::Table<Self::Key, Self::Value> {
        &mut self.0
    }
}

struct AssocTable<'db>(redb::Table<'db, &'db [u8], &'db [u8]>);
impl<'db> BashTable<'db> for AssocTable<'db> {
    type Key = &'db [u8];
    type Value = &'db [u8];
    fn open(txn: &mut WriteTxnGuard<'_>, name: &str) -> Result<Self, redb::Error> {
        Ok(Self(txn.open_assoc(name)?))
    }
    fn table(&mut self) -> &mut redb::Table<Self::Key, Self::Value> {
        &mut self.0
    }
}

/// Generic synchronization helper eliminating duplication between array and associative tables.
fn sync_bash_table<T, K, V, I>(
    write_txn: &mut WriteTxnGuard<'_>,
    name: &str,
    entries: I,
) -> Result<(), Box<dyn std::error::Error>>
where
    T: for<'db> BashTable<'db, Key = K, Value = V>,
    K: redb::Key + Ord + Eq + Clone,
    V: redb::Value,
    I: IntoIterator<Item = (K, V)>,
{
    let mut table_wrapper = T::open(write_txn, name)?;
    let table = table_wrapper.table();
    //
    let mut existing_keys = HashSet::new();
    for entry in table.iter()? {
        let (k, _) = entry?;
        existing_keys.insert(k.value().clone());
    }
    //
    let mut current_keys = HashSet::new();
    for (key, val) in entries {
        current_keys.insert(key.clone());
        table.insert(key, val)?;
    }
    //
    for stale_key in existing_keys.difference(&current_keys) {
        table.remove(stale_key)?;
    }
    Ok(())
}

unsafe fn dump_array_into_redb(
    write_txn: &mut WriteTxnGuard<'_>,
    array_name: &str,
    arr: *mut ARRAY,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = ArrayIterator::new(arr) .map(|(idx, cstr)| (idx, cstr.to_bytes()));
    sync_entries_to_redb::<ArrayTable<'_>, _, _, _>(write_txn, array_name, entries)
}

unsafe fn dump_assoc_into_redb(
    write_txn: &mut WriteTxnGuard<'_>,
    assoc_name: &str,
    hash: *mut HASH_TABLE,
) -> Result<(), Box<dyn std::error::Error>> {
    let entries = AssocIterator::new(hash).map(|(key_cstr, val_cstr)| (key_cstr.to_bytes(), val_cstr.to_bytes()));
    sync_entries_to_redb::<AssocTable<'_>, _, _, _>(write_txn, assoc_name, entries)
}

//////////////////////////////////////////////////////////////
// Read variable from database

unsafe fn load_array_into_bash(
    read_txn: &ReadTxnGuard<'_>,
    array_name: &str,
    arr: *mut ARRAY,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Iterate over the existing Bash array to collect all currently populated indices
    let mut existing_indices = HashSet::new();
    while let Some((idx, _)) in ArrayIterator::new(arr) {
        existing_indices.insert(idx);
    }
    // 2. Open the redb table and process incoming entries
    let mut current_indices = HashSet::new();
    let table = read_txn.open_array(array_name)?;
    for entry in table.iter()? {
        let (idx, val) = entry?;
        let idx_val = idx.value();
        let val_bytes = val.value();
        current_indices.insert(idx_val);
        unsafe {
            array_insert(arr, idx_val, val_bytes.as_ptr().cast());
        }
    }
    // 3. Remove indices that exist in Bash but are no longer present in the database
    let stale_indices = existing_indices.difference(&current_indices);
    for &stale_idx in stale_indices {
        unsafe {
            let ae = array_remove(arr, stale_idx);
        	array_dispose_element(ae);
        }
    }
    //
    Ok(())
}

unsafe fn load_assoc_into_bash(
    read_txn: &ReadTxnGuard<'_>,
    assoc_name: &str,
    hash: *mut HASH_TABLE,
) -> Result<(), Box<dyn std::error::Error>> {
    // 1. Iterate over the existing Bash array to collect all currently populated indices
    let mut existing_indices = HashSet::new();
    while let Some((idx, _)) in AssocIterator::new(arr) {
        existing_indices.insert(idx);
    }
    // 2. Open the redb table and process incoming entries
    let mut current_keys = HashSet::new();
    let table = read_txn.open_assoc(assoc_name)?;
    for entry in table.iter()? {
        let (key, val) = entry?;
        let key_bytes = key.value();
        let val_bytes = val.value();
        current_keys.insert(key_bytes);
        unsafe {
            l_assoc_insert(
                hash,
                key_bytes.as_ptr().cast(),
                val_bytes.as_ptr().cast(),
            );
        }
    }
    // 3. Remove indices that exist in Bash but are no longer present in the database
    for stale_key in existing_keys.difference(&current_keys) {
        unsafe {
            assoc_remove(stale_with_nul.as_ptr().cast(), hash, 0);
        }
    }
    Ok(())
}


