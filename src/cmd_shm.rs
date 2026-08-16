//! L_builtin `shm` subcommand: shared-memory variables backed by an LMDB
//! database (heed) stored in `/dev/shm`.
//!
//! The database holds multiple bash *array* variables (both indexed and
//! associative), keyed by the variable name. Each variable's full array is
//! serialized on every assignment and deserialized on every read, so the value
//! is shared across every process that maps the same `SHM_NAME` (for example a
//! background job started with `&`).
//!
//! Interface:
//!   L_builtin shm add [-A] SHM_NAME VAR_NAME
//!       Bind bash array variable VAR_NAME to the value stored under VAR_NAME
//!       in the shared-memory database SHM_NAME. With -A, create an associative
//!       array instead of an indexed array.
//!   L_builtin shm rm SHM_NAME [VAR_NAME...]
//!       Remove VAR_NAME(s) from the SHM_NAME database. With no VAR_NAME,
//!       remove the whole SHM_NAME database (and its backing files).
//!   L_builtin shm info SHM_NAME
//!       Print every variable stored in SHM_NAME.
//!
//! Example (indexed array):
//!   L_builtin shm add MYSHM MYVAR
//!   ( sleep 1; echo "${MYVAR[@]}" ) &
//!   MYVAR=( a b c )
//!   wait
//!   # the background job prints "a b c"
//!
//! Example (associative array):
//!   L_builtin shm add -A MYSHM MYVAR
//!   MYVAR=( [foo]=bar [baz]=qux )
//!   # shared across processes

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::os::raw::{c_char, c_int};
use std::path::{Path, PathBuf};

use crate::bash_api::{
    array_flush, array_insert, arrayind_t, assoc_flush, assoc_keys_to_word_list, assoc_reference,
    dispose_words, l_array_cell, l_array_head, l_assoc_cell, l_assoc_insert, l_element_forw,
    l_element_index, l_element_value, l_init_dynamic_array_var, l_init_dynamic_assoc_var,
    l_unbind_variable, this_cmd_name, variable, WordListView, ARRAY, EXECUTION_FAILURE,
    EXECUTION_SUCCESS, EX_USAGE, HASH_TABLE, SHELL_VAR, WORD_LIST,
};
use crate::subcmd::{CmdDesc, SubcommandFn};
use crate::{beprintln, bprintln, getopts, l_builtin_error, subcmd_getopts};

use heed::types::Bytes;
use heed::{Database, Env, MdbError};
use std::os::unix::ffi::OsStrExt;

/// Bash variable attribute for associative arrays (att_assoc).
/// From bash's variables.h: #define att_assoc 0x0000040
const ATT_ASSOC: c_int = 0x0000040;

/// Default LMDB map size (1 MiB). Grows automatically on `MDB_MAP_FULL`.
const INITIAL_MAP_SIZE: usize = 1024 * 1024;

/// Golden ratio used to grow the map when it fills up.
const GROWTH_FACTOR: f64 = 1.6180339887498949;

/// A cloneable LMDB environment handle.
type ShmEnv = Env<heed::WithTls>;
/// A cloneable database handle (default unnamed database, byte keys/values).
type ShmDb = Database<Bytes, Bytes>;

thread_local! {
    /// Registry: bash variable name -> SHM_NAME (which database it lives in).
    /// Stored per-thread because bash is single-threaded; interior mutation then
    /// needs no cross-thread lock, so `RefCell` gives safe mutability without a
    /// `Mutex` (which a `static` would otherwise require for its `Sync` bound).
    static REGISTRY: RefCell<HashMap<CString, LockedDatabase>> = RefCell::new(HashMap::new());
}

fn shm_dir(shm: &CStr) -> PathBuf {
    Path::new("/dev/shm")
        .join("l_builtin")
        .join(OsStr::from_bytes(shm.to_bytes()))
}

/// Build a NUL-terminated C string (as bytes) from a byte slice. The project
/// avoids dynamic string wrappers, so we use a plain `Vec<u8>` with a NUL.
fn to_cbytes(s: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(s.len() + 1);
    v.extend_from_slice(s);
    v.push(0);
    v
}

/// Open (or fetch from cache) the (Env, Db) for a SHM_NAME, creating the
/// backing directory and database on first use.
fn get_shm(shm: &CStr) -> Result<(ShmEnv, ShmDb), String> {
    {
        if let Some(e) = ENVS.with(|c| c.borrow().get(shm).map(|e| (e.0.clone(), e.1.clone()))) {
            return Ok(e);
        }
    }
    let dir = shm_dir(shm);
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("shm: cannot create {}: {}", dir.display(), e))?;
    let env = unsafe {
        heed::EnvOpenOptions::new()
            .map_size(INITIAL_MAP_SIZE)
            .max_dbs(1)
            .open(&dir)
    }
    .map_err(|e| format!("shm: cannot open env {}: {}", dir.display(), e))?;
    let mut wtxn = env
        .write_txn()
        .map_err(|e| format!("shm: write txn: {}", e))?;
    let db: ShmDb = env
        .create_database(&mut wtxn, None)
        .map_err(|e| format!("shm: create db: {}", e))?;
    wtxn.commit().map_err(|e| format!("shm: commit: {}", e))?;
    ENVS.with(|c| {
        c.borrow_mut()
            .insert(shm.to_owned(), (env.clone(), db.clone()))
    });
    Ok((env, db))
}

/// Grow `base` by the golden ratio and align the result up to a whole page.
/// The returned size is always strictly greater than `base`.
fn align_to_page_size(base: usize) -> usize {
    let page = unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as usize;
    let mut new_size = ((base as f64) * GROWTH_FACTOR).ceil() as usize;
    // Align up to a whole page.
    new_size = (new_size + page - 1) / page * page;
    if new_size <= base {
        new_size = base + page;
    }
    new_size
}

/// Put a value, growing the LMDB map on `MDB_MAP_FULL` until it fits.
///
/// The write transaction is opened once and held across the resize. LMDB's
/// writer mutex (held by that transaction) serializes the resize across all
/// processes mapping the same environment, so no external lock is needed. The
/// `put` is retried on the same transaction after each growth.
fn put_value(env: &ShmEnv, db: &ShmDb, key: &[u8], value: &[u8]) -> Result<(), String> {
    let mut wtxn = env
        .write_txn()
        .map_err(|e| format!("shm: write txn: {}", e))?;
    let mut attempt = 0;
    loop {
        attempt += 1;
        if attempt > 64 {
            return Err("shm: map resize limit exceeded".into());
        }
        match db.put(&mut wtxn, key, value) {
            Ok(()) => {
                wtxn.commit().map_err(|e| format!("shm: commit: {}", e))?;
                return Ok(());
            }
            Err(heed::Error::Mdb(MdbError::MapFull)) => {
                let new_size = align_to_page_size(env.info().map_size);
                unsafe {
                    env.resize(new_size)
                        .map_err(|e| format!("shm: resize: {}", e))?
                };
                // Map grew while the txn is still open; retry the put on it.
            }
            Err(e) => return Err(format!("shm: put: {}", e)),
        }
    }
}

/// On-disk layout for one array variable: a sequence of records
/// `[index: i64 LE][value bytes][NUL]`. The NUL terminator lets us hand a raw
/// pointer straight into `array_insert` (which copies the string), so no
/// intermediate (index, bytes) vector is needed.
///
/// Iterate the records, calling `f(index, value)` for each. `value` points at a
/// NUL-terminated string inside `buf`, so it may be passed directly to a C
/// function that copies it.
fn each_element(buf: &[u8], mut f: impl FnMut(arrayind_t, &[u8])) {
    let mut off = 0;
    while off + 8 <= buf.len() {
        let idx = arrayind_t::from_le_bytes(buf[off..off + 8].try_into().unwrap());
        off += 8;
        let start = off;
        let end = match buf[off..].iter().position(|&b| b == 0) {
            Some(p) => off + p,
            // Missing terminator: treat the rest as the (unterminated) value.
            None => buf.len(),
        };
        f(idx, &buf[start..end]);
        off = end + 1;
    }
}

/// On-disk layout for one associative array variable: a sequence of records
/// `[key_len: u32 LE][key bytes][value bytes][NUL]`. The value is NUL-terminated;
/// the key is NOT NUL-terminated in the stored format (its length is given by
/// `key_len`). When loading, we create a temporary NUL-terminated copy of the
/// key for `l_assoc_insert` (which expects a NUL-terminated key string).
///
/// Iterate the records, calling `f(key, value)` for each. `key` is a slice of
/// `key_len` bytes (NOT NUL-terminated). `value` points at a NUL-terminated
/// string inside `buf`.
fn each_assoc_element(buf: &[u8], mut f: impl FnMut(&[u8], &[u8])) {
    let mut off = 0;
    while off + 4 <= buf.len() {
        let key_len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if off + key_len > buf.len() {
            break;
        }
        let key = &buf[off..off + key_len];
        off += key_len;
        let start = off;
        let end = match buf[off..].iter().position(|&b| b == 0) {
            Some(p) => off + p,
            // Missing terminator: treat the rest as the (unterminated) value.
            None => buf.len(),
        };
        let val = &buf[start..end];
        f(key, val);
        off = end + 1;
    }
}

/// Load an associative array variable's on-disk buffer directly into a bash
/// HASH_TABLE, inserting each record in the same loop. Flushes the hash first.
unsafe fn load_assoc_into_bash(hash: *mut HASH_TABLE, buf: &[u8]) {
    assoc_flush(hash);
    each_assoc_element(buf, |key, val| {
        // `key` is NOT NUL-terminated in the stored format, but `l_assoc_insert`
        // expects a NUL-terminated key (it uses l_strdup/strlen). Create a
        // temporary NUL-terminated copy.
        let mut key_with_nul = Vec::with_capacity(key.len() + 1);
        key_with_nul.extend_from_slice(key);
        key_with_nul.push(0);
        // `val` IS NUL-terminated by the record's terminator.
        unsafe {
            l_assoc_insert(
                hash,
                key_with_nul.as_ptr() as *const c_char,
                val.as_ptr() as *const c_char,
            )
        };
    });
}

/// Dump a bash HASH_TABLE straight into an on-disk buffer: key length followed
/// by the key, then the NUL-terminated value, for every entry.
unsafe fn dump_assoc_from_bash(hash: *mut HASH_TABLE) -> Vec<u8> {
    let mut out = Vec::new();
    let keys = assoc_keys_to_word_list(hash);
    if keys.is_null() {
        return out;
    }
    let mut wl = keys;
    while !wl.is_null() {
        let word_ptr = (*wl).word;
        if !word_ptr.is_null() {
            let key_ptr = (*word_ptr).word;
            if !key_ptr.is_null() {
                let key = CStr::from_ptr(key_ptr).to_bytes();
                let val_ptr = assoc_reference(hash, key_ptr);
                let val = if val_ptr.is_null() {
                    &[][..]
                } else {
                    CStr::from_ptr(val_ptr).to_bytes()
                };
                out.extend_from_slice(&(key.len() as u32).to_le_bytes());
                out.extend_from_slice(key);
                out.extend_from_slice(val);
                out.push(0);
            }
        }
        wl = (*wl).next;
    }
    dispose_words(keys);
    out
}

/// Load an array variable's on-disk buffer directly into a bash ARRAY,
/// inserting each record in the same loop. Flushes the array first.
unsafe fn load_array_into_bash(arr: *mut ARRAY, buf: &[u8]) {
    array_flush(arr);
    each_element(buf, |idx, val| {
        // `val` is NUL-terminated by the record's terminator, so `val.as_ptr()`
        // is a valid C string `array_insert` will copy.
        unsafe { array_insert(arr, idx, val.as_ptr() as *mut c_char) };
    });
}

/// Dump a bash ARRAY straight into an on-disk buffer: index followed by the
/// NUL-terminated value, for every real element (skipping the circular dummy
/// head, whose `ind` is -1).
unsafe fn dump_array_from_bash(arr: *mut ARRAY) -> Vec<u8> {
    let mut out = Vec::new();
    let head = l_array_head(arr);
    if head.is_null() {
        return out;
    }
    let mut ae = l_element_forw(head);
    while ae != head {
        let idx = l_element_index(ae);
        out.extend_from_slice(&idx.to_le_bytes());
        let val = l_element_value(ae);
        if !val.is_null() {
            out.extend_from_slice(CStr::from_ptr(val).to_bytes());
        }
        out.push(0);
        ae = l_element_forw(ae);
    }
    out
}

/// Highest index present in a bash ARRAY, plus one (for append semantics).
unsafe fn bash_array_append_index(arr: *mut ARRAY) -> arrayind_t {
    let head = l_array_head(arr);
    if head.is_null() {
        return 0;
    }
    let mut max = -1 as arrayind_t;
    let mut ae = l_element_forw(head);
    while ae != head {
        let idx = l_element_index(ae);
        if idx > max {
            max = idx;
        }
        ae = l_element_forw(ae);
    }
    if max < 0 {
        0
    } else {
        max + 1
    }
}

/// Read the raw on-disk buffer for a variable out of LMDB (empty if absent).
fn read_raw_bytes(env: &ShmEnv, db: &ShmDb, key: &[u8]) -> Vec<u8> {
    match env.read_txn() {
        Ok(rtxn) => match db.get(&rtxn, key) {
            Ok(Some(b)) => b.to_vec(),
            _ => Vec::new(),
        },
        Err(_) => Vec::new(),
    }
}

/// Dynamic-array getter: resolve VAR_NAME from LMDB and rebuild the array.
unsafe extern "C" fn shm_getter(var: *mut variable) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let shm_name = match REGISTRY.with(|r| r.borrow().get(var_name).cloned()) {
        Some(s) => s,
        None => return var,
    };
    let (env, db) = match get_shm(&shm_name) {
        Ok(x) => x,
        Err(_) => return var,
    };
    let rtxn = match env.read_txn() {
        Ok(t) => t,
        Err(_) => return var,
    };
    let value = match db.get(&rtxn, var_name.to_bytes()) {
        Ok(Some(b)) => Some(b),
        Ok(None) => None,
        Err(_) => return var,
    };
    let arr = l_array_cell(var as *mut SHELL_VAR);
    load_array_into_bash(
        arr,
        match value {
            Some(b) => &b,
            // Missing key: flush any stale contents to an empty array.
            None => &[],
        },
    );
    var
}

/// Dynamic-array setter: read the current array from LMDB, apply the single
/// assigned element, persist the whole array back, and rebuild the local bash
/// array so in-process reads stay consistent. Matches bash's
/// `sh_var_assign_func_t`: `(var, value, ind, key)`.
unsafe extern "C" fn shm_array_setter(
    var: *mut variable,
    value: *mut c_char,
    ind: arrayind_t,
    _key: *mut c_char,
) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let shm_name = match REGISTRY.with(|r| r.borrow().get(var_name).cloned()) {
        Some(s) => s,
        None => return var,
    };
    if let Ok((env, db)) = get_shm(&shm_name) {
        // Read the current shared state, rebuild the local array, then apply
        // the single assigned element (replace-or-append).
        let existing = read_raw_bytes(&env, &db, var_name.to_bytes());
        let arr = l_array_cell(var as *mut SHELL_VAR);
        load_array_into_bash(arr, &existing);
        let idx = if ind < 0 {
            bash_array_append_index(arr)
        } else {
            ind
        };
        // `value` is already NUL-terminated by bash; `array_insert` copies it,
        // so pass it straight through. A null value becomes the empty string.
        let cval: *mut c_char = if value.is_null() {
            b"\0".as_ptr() as *mut c_char
        } else {
            value
        };
        array_insert(arr, idx, cval);
        let serialized = dump_array_from_bash(arr);
        let _ = put_value(&env, &db, var_name.to_bytes(), &serialized);
    }
    var
}

/// Dynamic-associative-array getter: resolve VAR_NAME from LMDB and rebuild the
/// associative array.
unsafe extern "C" fn shm_assoc_getter(var: *mut variable) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let shm_name = match REGISTRY.with(|r| r.borrow().get(var_name).cloned()) {
        Some(s) => s,
        None => return var,
    };
    let (env, db) = match get_shm(&shm_name) {
        Ok(x) => x,
        Err(_) => return var,
    };
    let rtxn = match env.read_txn() {
        Ok(t) => t,
        Err(_) => return var,
    };
    let value = match db.get(&rtxn, var_name.to_bytes()) {
        Ok(Some(b)) => Some(b),
        Ok(None) => None,
        Err(_) => return var,
    };
    let hash = l_assoc_cell(var as *mut SHELL_VAR);
    load_assoc_into_bash(
        hash,
        match value {
            Some(b) => &b,
            // Missing key: flush any stale contents to an empty hash.
            None => &[],
        },
    );
    var
}

/// Dynamic-associative-array setter: read the current hash from LMDB, apply the
/// single assigned key-value pair, persist the whole hash back, and rebuild the
/// local bash hash so in-process reads stay consistent. Matches bash's
/// `sh_var_assign_func_t`: `(var, value, ind, key)`. For associative arrays,
/// `ind` is ignored and `key` is the string key.
unsafe extern "C" fn shm_assoc_setter(
    var: *mut variable,
    value: *mut c_char,
    _ind: arrayind_t,
    key: *mut c_char,
) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let shm_name = match REGISTRY.with(|r| r.borrow().get(var_name).cloned()) {
        Some(s) => s,
        None => return var,
    };
    if let Ok((env, db)) = get_shm(&shm_name) {
        // Read the current shared state, rebuild the local hash, then apply
        // the single assigned key-value pair.
        let existing = read_raw_bytes(&env, &db, var_name.to_bytes());
        let hash = l_assoc_cell(var as *mut SHELL_VAR);
        load_assoc_into_bash(hash, &existing);
        // `key` and `value` are already NUL-terminated by bash; `l_assoc_insert`
        // copies them, so pass them straight through. A null value becomes the
        // empty string.
        let cval: *const c_char = if value.is_null() {
            b"\0".as_ptr() as *const c_char
        } else {
            value
        };
        let ckey: *const c_char = if key.is_null() {
            b"\0".as_ptr() as *const c_char
        } else {
            key
        };
        unsafe { l_assoc_insert(hash, ckey, cval) };
        let serialized = dump_assoc_from_bash(hash);
        let _ = put_value(&env, &db, var_name.to_bytes(), &serialized);
    }
    var
}

/// Try to detect if the stored data is an associative array format.
/// Returns true if the data appears to be associative array format.
fn is_assoc_format(buf: &[u8]) -> bool {
    let mut off = 0;
    while off + 4 <= buf.len() {
        let key_len = u32::from_le_bytes(buf[off..off + 4].try_into().unwrap()) as usize;
        off += 4;
        if key_len > 4096 {
            // Unreasonably large key, probably not assoc format
            return false;
        }
        if off + key_len > buf.len() {
            return false;
        }
        off += key_len;
        // Find NUL terminator for value
        if off >= buf.len() {
            return false;
        }
        let val_end = match buf[off..].iter().position(|&b| b == 0) {
            Some(p) => off + p,
            None => return false,
        };
        off = val_end + 1;
    }
    off == buf.len()
}
fn remove_shm(shm: &CStr) {
    REGISTRY.with(|r| r.borrow_mut().retain(|_, v| v != shm));
    ENVS.with(|c| c.borrow_mut().remove(shm));
    let _ = std::fs::remove_dir_all(shm_dir(shm));
}

/// Escape a byte string for inclusion inside double quotes.
fn escape_quoted(out: &mut Vec<u8>, bytes: &[u8]) {
    out.push(b'"');
    for &b in bytes {
        match b {
            b'\\' | b'"' => {
                out.push(b'\\');
                out.push(b);
            }
            _ => out.push(b),
        }
    }
    out.push(b'"');
}

/// `L_builtin shm add [-A] SHM_NAME VAR_NAME`
unsafe extern "C" fn shm_add_subcommand(list: *mut WORD_LIST) -> c_int {
    let mut is_assoc = false;
    let (shm, var) = subcmd_getopts!(
        SHM_ADD_CMD,
        list,
        flags: [ A => || is_assoc = true ],
        required: [SHM_NAME, VAR_NAME],
    );
    let shm_str = shm.as_cstr();
    let var_str = var.as_cstr();
    if let Err(e) = get_shm(&shm_str) {
        beprintln!(this_cmd_name(), b": ", e.as_bytes());
        return EXECUTION_FAILURE;
    }
    REGISTRY.with(|r| {
        r.borrow_mut()
            .insert(var_str.to_owned(), shm_str.to_owned())
    });
    let result = if is_assoc {
        l_init_dynamic_assoc_var(
            var.as_ptr(),
            Some(shm_assoc_getter),
            Some(shm_assoc_setter),
            ATT_ASSOC,
        )
    } else {
        l_init_dynamic_array_var(var.as_ptr(), Some(shm_getter), Some(shm_array_setter), 0)
    };
    if result.is_null() {
        beprintln!(this_cmd_name(), b": failed to bind variable");
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}

/// `L_builtin shm rm SHM_NAME [VAR_NAME...]`
unsafe extern "C" fn shm_rm_subcommand(list: *mut WORD_LIST) -> c_int {
    let (shm, vars) = subcmd_getopts!(SHM_RM_CMD, list, required: [SHM_NAME], rest: VARS);
    let shm_str = shm.as_cstr();
    if vars.is_empty() {
        remove_shm(&shm_str);
        return EXECUTION_SUCCESS;
    }
    let (env, db) = match get_shm(&shm_str) {
        Ok(x) => x,
        Err(e) => {
            l_builtin_error!(e.as_bytes());
            return EXECUTION_FAILURE;
        }
    };
    let mut wtxn = match env.write_txn() {
        Ok(t) => t,
        Err(e) => {
            l_builtin_error!(e.to_string());
            return EXECUTION_FAILURE;
        }
    };
    for v in &vars {
        let name = v.as_cstr();
        if db.delete(&mut wtxn, name.to_bytes()).is_err() {
            // Not present; ignore.
        }
        REGISTRY.with(|r| r.borrow_mut().remove(name));
        l_unbind_variable(name.as_ptr() as *const c_char);
    }
    if wtxn.commit().is_err() {
        l_builtin_error!("commit failed");
        return EXECUTION_FAILURE;
    }
    EXECUTION_SUCCESS
}

/// `L_builtin shm info SHM_NAME`
unsafe extern "C" fn shm_info_subcommand(list: *mut WORD_LIST) -> c_int {
    let (shm,) = subcmd_getopts!(SHM_INFO_CMD, list, required: [SHM_NAME]);
    let shm_str = shm.as_cstr();
    let (env, db) = match get_shm(&shm_str) {
        Ok(x) => x,
        Err(e) => {
            l_builtin_error!(e.as_bytes());
            return EXECUTION_FAILURE;
        }
    };
    let rtxn = match env.read_txn() {
        Ok(t) => t,
        Err(e) => {
            l_builtin_error!(e.to_string());
            return EXECUTION_FAILURE;
        }
    };
    let iter = match db.iter(&rtxn) {
        Ok(i) => i,
        Err(e) => {
            l_builtin_error!(e.to_string());
            return EXECUTION_FAILURE;
        }
    };
    for item in iter {
        let (key, value) = match item {
            Ok(x) => x,
            Err(_) => continue,
        };
        let mut line: Vec<u8> = Vec::new();
        line.extend_from_slice(key);
        line.extend_from_slice(b"=(");
        let mut first = true;
        if is_assoc_format(value) {
            each_assoc_element(value, |k, val| {
                if !first {
                    line.push(b' ');
                }
                first = false;
                line.extend_from_slice(b"[");
                escape_quoted(&mut line, k);
                line.extend_from_slice(b"]=");
                escape_quoted(&mut line, val);
            });
        } else {
            each_element(value, |idx, val| {
                if !first {
                    line.push(b' ');
                }
                first = false;
                line.extend_from_slice(format!("[{}]=", idx).as_bytes());
                escape_quoted(&mut line, val);
            });
        }
        line.extend_from_slice(b")");
        bprintln!(line);
    }
    EXECUTION_SUCCESS
}

const SHM_CMD: CmdDesc = CmdDesc::new(
    c"shm",
    c"add [-A] SHM_NAME VAR_NAME | rm SHM_NAME [VAR_NAME...] | info SHM_NAME",
    c"\
Shared-memory variables backed by an LMDB database in /dev/shm.

Subcommands:
  add [-A] SHM_NAME VAR_NAME
                          Bind bash array variable VAR_NAME to the value
                          stored under VAR_NAME in the SHM_NAME database.
                          With -A, create an associative array instead of an
                          indexed array.
  rm  SHM_NAME [VAR_NAME...]
                          Remove VAR_NAME(s) from the SHM_NAME database. With no
                          VAR_NAME, remove the whole SHM_NAME database.
  info SHM_NAME           Print every variable stored in SHM_NAME.

The variable (indexed or associative array) is serialized into LMDB on every
assignment and is visible to every process that maps the same SHM_NAME (e.g. a
background job started with &). The database grows automatically when full.
",
);

const SHM_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("add", shm_add_subcommand),
    ("rm", shm_rm_subcommand),
    ("info", shm_info_subcommand),
];

const SHM_ADD_CMD: CmdDesc = CmdDesc::new(
    c"add",
    c"add [-A] SHM_NAME VAR_NAME",
    c"\
Bind a bash array variable to a shared-memory database.

VAR_NAME becomes a dynamic array whose value is stored under VAR_NAME
in the LMDB database for SHM_NAME (in /dev/shm). Every assignment is written
through to LMDB and is visible to every process that maps the same SHM_NAME,
e.g. a background job started with &.

With -A, create an associative array (key-value pairs with string keys)
instead of an indexed array (integer indices).

Examples:
  L_builtin shm add mydb v
  v=(a b c)          # indexed array, stored in 'mydb', shared across processes
  v[0]=changed       # a single-index write is visible to other processes

  L_builtin shm add -A mydb v
  v=( [foo]=bar [baz]=qux )  # associative array, shared across processes
  v[foo]=changed     # a single-key write is visible to other processes
",
);

const SHM_RM_CMD: CmdDesc = CmdDesc::new(
    c"rm",
    c"rm SHM_NAME [VAR_NAME...]",
    c"\
Remove variable(s) from a shared-memory database.

With one or more VAR_NAME arguments, only those variables are removed from the
SHM_NAME database (the variable is unbound in the shell). With no VAR_NAME, the
entire SHM_NAME database and its backing files in /dev/shm are removed.

Examples:
  L_builtin shm rm mydb v        # remove one variable
  L_builtin shm rm mydb v w      # remove several variables
  L_builtin shm rm mydb          # remove the whole database
",
);

const SHM_INFO_CMD: CmdDesc = CmdDesc::new(
    c"info",
    c"info SHM_NAME",
    c"\
Print every variable stored in a shared-memory database.

The output is a series of bash array assignments, one per variable, that can be
eval'd to reconstruct the shared state.

Examples:
  L_builtin shm info mydb
",
);

const SHM_TABLE: crate::intlookup::U32::IntLookup<SubcommandFn, 3> =
    crate::intlookup!(&SHM_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
#[no_mangle]
pub unsafe extern "C" fn shm_subcommand(list: *mut WORD_LIST) -> c_int {
    SHM_CMD.enter();
    let rest = getopts!(list, [], []);
    let mut iter = WordListView::from_raw(rest).into_iter();
    let action = match iter.next() {
        Some(a) => a,
        None => {
            beprintln!(this_cmd_name(), b": usage: L_builtin shm <add|rm|info> ...");
            return EX_USAGE;
        }
    };
    let action_bytes = action.as_bytes();
    let handler = match SHM_TABLE.lookup(action_bytes) {
        Some(h) => h,
        None => {
            beprintln!(this_cmd_name(), b": unknown shm subcommand: ", action_bytes);
            return EX_USAGE;
        }
    };
    handler(iter.as_ptr())
}
