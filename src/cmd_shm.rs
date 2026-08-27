//! L_builtin `shm` subcommand: shared-memory variables backed by a `rkyv`
//! database.
//!
//! Each bound bash variable gets an entry in the shared database keyed by its
//! bare name (see [`crate::vardb`]). Indexed arrays are stored as an
//! index-to-value map; associative arrays as a key-to-value map. Every element
//! value is stored without its trailing NUL; the load path re-adds the NUL when
//! handing the string to bash.
//!
//! The value is shared across every process that maps the same database (for
//! example a background job started with `&`): every assignment is written
//! through to the rkyv blob and a read refreshes the local bash variable from
//! the blob.
//!
//! The database is selected by one of `-s NAME` (POSIX shared memory),
//! `-n NAME` (anonymous in-memory mapping), `-M NAME:SIZE` (anonymous
//! fixed-size mmap, created with a size and then referenced by NAME),
//! `-F PATH` (regular file); with none of them the default in-memory mapping
//! named `DEFAULT` is used.
//!
//! Example (indexed array, default database):
//!   L_builtin shm bind MYVAR
//!   ( sleep 1; echo "${MYVAR[@]}" ) &
//!   MYVAR=( a b c )
//!   wait
//!   # the background job prints "a b c"
//!
//! Example (associative array, POSIX shared memory):
//!   L_builtin shm bind -A -s MYSHM MYVAR
//!   MYVAR=( [foo]=bar [baz]=qux )
//!   # shared across processes

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::collections::HashMap;
use std::ffi::{CStr, CString, OsStr};
use std::os::raw::{c_char, c_int};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;
use std::sync::Arc;

use cmdargs_derive::CmdArgs;

use crate::bash_api::{
    array_insert, array_remove, arrayind_t, assoc_remove, find_variable, is_valid_var_name,
    l_array_cell, l_array_max_index, l_assoc_cell, l_assoc_insert, l_assoc_p,
    l_init_dynamic_array_var, l_init_dynamic_assoc_var, l_unbind_variable, variable, ArrayIterator,
    AssocIterator, WordListIterCpnt, EXECUTION_FAILURE, EX_USAGE, SHELL_VAR, WORD_LIST,
};
use crate::subcmd::{CmdDesc, CmdResult, SubCommandCallerArgs, SubcommandFn};
use crate::vardb::{open_db_loc, DbPath, LockedDatabase, VarData};
use crate::{beprintln, bprintln, l_builtin_error, l_builtin_usage_error};

thread_local! {
    static REGISTRY: std::cell::RefCell<HashMap<CString, Arc<LockedDatabase>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Find (or open) the existing database for `loc`: file-backed and POSIX-shm
/// databases are reopened by name/path, while an in-memory `Mem` database can
/// only be shared with forked children and so must already be bound (it is found
/// through the registry).
fn get_db_loc(loc: &DbPath) -> Result<Arc<LockedDatabase>, String> {
    match loc {
        DbPath::Mem(name) => REGISTRY.with(|r| {
            r.borrow()
                .values()
                .find(|db| matches!(&db.path(), DbPath::Mem(n) if n.as_bytes() == name.to_bytes()))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "shm: no anonymous database named {}",
                        name.to_str().unwrap_or("")
                    )
                })
        }),
        DbPath::Mmap { name, .. } => REGISTRY.with(|r| {
            r.borrow()
                .values()
                .find(|db| matches!(&db.path(), DbPath::Mmap { name: n, .. } if n.as_bytes() == name.to_bytes()))
                .cloned()
                .ok_or_else(|| {
                    format!("shm: no anonymous mmap database named {}", name.to_string_lossy())
                })
        }),
        DbPath::Shm(_) | DbPath::File(_) => open_db_loc(loc).map(Arc::new),
    }
}

/// A stable key for a database location, matching the keys produced by
/// [`db_key`] for an opened database.
fn loc_key(loc: &DbPath) -> String {
    db_key(loc)
}

/// Unlink the backing object/file for a database location. An in-memory (memfd)
/// or anonymous-mmap database has no backing object to unlink; its data
/// disappears when the last reference is dropped.
fn unlink_db_backing(loc: &DbPath) {
    match loc {
        DbPath::File(p) => {
            let _ = std::fs::remove_file(p);
        }
        DbPath::Shm(name) => LockedDatabase::unlink_shm(name),
        DbPath::Mem(_) | DbPath::Mmap { .. } => {}
    }
}

/// Unbind every bash variable in this shell that is bound to the database with
/// the given key, and drop the corresponding REGISTRY entries. The database
/// contents themselves are left untouched.
fn unbind_registry_vars(key: &str) {
    let mut to_remove: Vec<CString> = Vec::new();
    REGISTRY.with(|r| {
        let reg = r.borrow();
        for (var, db) in reg.iter() {
            if db_key(&db.path()) == key {
                to_remove.push(var.clone());
            }
        }
    });
    for var in to_remove {
        unsafe { l_unbind_variable(var.as_ptr() as *const c_char) };
        REGISTRY.with(|r| {
            r.borrow_mut().remove(var.as_c_str());
        });
    }
}

/// Fetch the database registered for `var`. The handle always reloads the blob
/// from disk, so no fork/pid handling is needed.
///
/// This is called from the dynamic variable getter/setter callbacks, which run
/// outside any builtin command context (so `this_cmd_name()` is meaningless); on
/// failure it prints a hard-coded, builtin-scoped message and returns `None`.
fn get_shm(var: &CStr) -> Option<Arc<LockedDatabase>> {
    REGISTRY.with(|r| {
        let reg = r.borrow();
        let db = reg.get(var);
        if db.is_none() {
            beprintln!(
                b"L_builtin: shm: variable ",
                var.to_bytes(),
                b" is not bound to a shared database; bind it with `L_builtin shm bind \
<NAME>`, or it was unbound/removed (shm unbind / shm rm) while the shell variable still exists",
            );
        }
        db.cloned()
    })
}

/// Return `p` unchanged, or a pointer to an empty NUL string when `p` is null.
fn ptr_or_empty(p: *const c_char) -> *const c_char {
    if p.is_null() {
        b"\0".as_ptr() as *const c_char
    } else {
        p
    }
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

macro_rules! my_debug {
    ($($arg:tt)*) => {{
        #[cfg(test)]
        bprintln!($($arg)*);
    }};
}

/// Dynamic-array getter: rebuild the bash array from the shared database.
/// Stateful: snapshot the indices bash currently holds and only insert/update
/// the entries whose database value differs, then remove the indices the
/// database no longer contains. This avoids reallocating the whole array on
/// every read (see array_flush/rebuild discussion in the issue).
unsafe extern "C" fn shm_array_getter(var: *mut variable) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let db = match get_shm(var_name) {
        Some(x) => x,
        None => return var,
    };
    if let Ok(repr) = db.read() {
        let arr = l_array_cell(var as *mut SHELL_VAR);
        // Snapshot the indices bash currently holds (by reference, no copy), so we
        // can leave unchanged entries alone and drop the ones the database no
        // longer contains.
        let mut existing: HashMap<i64, &'static CStr> = HashMap::new();
        for (idx, val) in ArrayIterator::new(arr) {
            existing.insert(idx, val);
        }
        if let Some(map) = repr.vars.get(var_name).and_then(|v| v.as_array()) {
            for (idx, val) in map {
                let val_bytes = val.to_bytes();
                // `existing` holds the bash value; drop it and insert only when the
                // database value differs (new keys count as changed).
                let changed = match existing.remove(idx) {
                    Some(cur) => cur.to_bytes() != val_bytes,
                    None => true,
                };
                if changed {
                    my_debug!("shm_array_getter: ", var_name, "[", idx, "]=", val.as_ptr());
                    array_insert(arr, *idx, val.as_ptr() as *mut c_char);
                }
            }
        }
        // Whatever remains was deleted from the database.
        for idx in existing.keys() {
            my_debug!("shm_array_getter: unset ", var_name, "[", idx, "]");
            array_remove(arr, *idx);
        }
    }
    var
}

/// Dynamic-array setter: update the local bash array, then persist the single
/// assigned element to the shared database. Matches bash's
/// `sh_var_assign_func_t`: `(var, value, ind, key)`.
unsafe extern "C" fn shm_array_setter(
    var: *mut variable,
    value: *mut c_char,
    ind: arrayind_t,
    _key: *mut c_char,
) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let db = match get_shm(var_name) {
        Some(x) => x,
        None => return var,
    };
    let name = var_name.to_owned();
    let arr = l_array_cell(var as *mut SHELL_VAR);
    let idx = if ind < 0 {
        l_array_max_index(arr) + 1
    } else {
        ind
    };
    // `value` is already NUL-terminated by bash; `array_insert` copies it.
    let cval_ptr: *mut c_char = ptr_or_empty(value as *const c_char) as *mut c_char;
    array_insert(arr, idx, cval_ptr);
    let cval = CStr::from_ptr(cval_ptr as *const c_char).to_owned();
    let _ = db.with_write(move |repr| {
        let vd = repr.vars.entry(name).or_insert_with(|| VarData::default());
        vd.insert_index(idx, cval);
    });
    var
}

/// Dynamic-associative-array getter: rebuild the associative array from the
/// shared database.
unsafe extern "C" fn shm_assoc_getter(var: *mut variable) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let db = match get_shm(var_name) {
        Some(x) => x,
        None => return var,
    };
    if let Ok(repr) = db.read() {
        let hash = l_assoc_cell(var as *mut SHELL_VAR);
        // Snapshot the keys bash currently holds (by reference, no copy), so we can
        // leave unchanged entries alone and drop the ones the database no longer
        // contains.
        let mut existing: HashMap<&'static CStr, &'static CStr> = HashMap::new();
        for (k, v) in AssocIterator::new(hash) {
            existing.insert(k, v);
        }
        if let Some(map) = repr.vars.get(var_name).and_then(|v| v.as_assoc()) {
            for (k, v) in map {
                let v_bytes = v.to_bytes();
                // `existing` holds the bash value; drop it and insert only when the
                // database value differs (new keys count as changed).
                let changed = match existing.remove(k.as_c_str()) {
                    Some(cur) => cur.to_bytes() != v_bytes,
                    None => true,
                };
                if changed {
                    l_assoc_insert(
                        hash,
                        k.as_ptr() as *const c_char,
                        v.as_ptr() as *const c_char,
                    );
                }
            }
        }
        // Whatever remains was deleted from the database. The keys are
        // NUL-terminated (they came straight from bash's assoc), so pass the
        // pointer through directly.
        for (k, _v) in existing {
            assoc_remove(hash, k.as_ptr() as *mut c_char);
        }
    }
    var
}

/// Dynamic-associative-array setter: update the local bash hash, then persist
/// the single assigned key-value pair to the shared database. Matches bash's
/// `sh_var_assign_func_t`: `(var, value, ind, key)`. For associative arrays,
/// `ind` is ignored and `key` is the string key.
unsafe extern "C" fn shm_assoc_setter(
    var: *mut variable,
    value: *mut c_char,
    _ind: arrayind_t,
    key: *mut c_char,
) -> *mut variable {
    let var_name = CStr::from_ptr((*var).name);
    let db = match get_shm(var_name) {
        Some(x) => x,
        None => return var,
    };
    let name = var_name.to_owned();
    let hash = l_assoc_cell(var as *mut SHELL_VAR);
    // `key` and `value` are already NUL-terminated by bash; `l_assoc_insert`
    // copies them.
    let ckey_ptr: *const c_char = ptr_or_empty(key);
    let cval_ptr: *const c_char = ptr_or_empty(value);
    l_assoc_insert(hash, ckey_ptr, cval_ptr);
    let ckey = CStr::from_ptr(ckey_ptr).to_owned();
    let cval = CStr::from_ptr(cval_ptr).to_owned();
    let _ = db.with_write(move |repr| {
        let vd = repr.vars.entry(name).or_insert_with(|| VarData::default());
        vd.insert_key(ckey, cval);
    });
    var
}

/// Shared backing flags (`-s`/`-n`/`-M`/`-F`) for the `shm` subcommands that
/// only select a database. `post()` enforces that at most one of them is given.
#[derive(CmdArgs)]
struct ShmLocArgs {
    /// POSIX shared memory object name (`-s NAME`).
    #[opt('s')]
    s: Option<&'static CStr>,
    /// Anonymous in-memory mapping name (`-n NAME`).
    #[opt('n')]
    n: Option<&'static CStr>,
    /// Named anonymous mmap (`-M NAME` to select, `-M NAME:SIZE` to create).
    #[opt('M')]
    m: Option<&'static CStr>,
    /// Regular file path (`-F PATH`).
    #[opt('F')]
    f: Option<&'static CStr>,
}

/// `shm bind` arguments: the shared backing flags plus `-A` (associative) and the
/// required `VAR_NAME...` positionals.
#[derive(CmdArgs)]
struct ShmBindArgs {
    /// Backing selection flags, shared with the other `shm` subcommands.
    #[flatten]
    loc: ShmLocArgs,
    /// Create an associative array instead of an indexed array.
    #[flag('A')]
    assoc: bool,
    /// Bash variable names to bind (one or more).
    #[rest]
    vars: Vec<BashVar>,
}

/// `shm sync` arguments: the shared backing flags plus `VAR_NAME...` to push.
#[derive(CmdArgs)]
struct ShmSyncArgs {
    /// Backing selection flags, shared with the other `shm` subcommands.
    #[flatten]
    loc: ShmLocArgs,
    /// Bash variables to push into the shared database.
    #[rest]
    vars: Vec<BashVar>,
}

/// `shm unbind VAR_NAME...`: var-located. Find each variable's store via the
/// registry and drop the local binding. No backing flags needed.
#[derive(CmdArgs)]
struct ShmUnbindArgs {
    /// Bash variable names to unbind (one or more).
    #[rest]
    vars: WordListIterCpnt<'static>,
}

/// `shm drop VAR_NAME...`: remove each variable's data from its store (and drop
/// the local binding). Var-located - the store is found via each variable's
/// binding, so no backing flags are needed/used.
#[derive(CmdArgs)]
struct ShmDropArgs {
    /// Bash variable names whose stored data is removed.
    #[rest]
    vars: Vec<BashVar>,
}

/// `shm clear [BACKING]`: wipe every variable's data from a store, leaving the
/// backing object in place. The store is selected by the backing flags (or the
/// default `DEFAULT` store).
#[derive(CmdArgs)]
struct ShmClearArgs {
    /// Backing selection flags, shared with the other `shm` subcommands.
    #[flatten]
    loc: ShmLocArgs,
}

/// Validate that at most one of `-s`, `-n`, `-M`, `-F` was supplied. Shared by
/// every `shm` args struct via its inherent `post` implementation.
impl ShmLocArgs {
    fn post(&self) -> CmdResult {
        let n = [
            self.s.is_some(),
            self.n.is_some(),
            self.m.is_some(),
            self.f.is_some(),
        ]
        .iter()
        .filter(|&&b| b)
        .count();
        if n > 1 {
            return Err(l_builtin_usage_error!(b"shm: -s, -n, -M and -F are mutually exclusive"));
        }
        Ok(())
    }

    fn resolve_dbloc(&self) -> Result<DbPath, c_int> {
        let loc = if let Some(p) = self.f {
            DbPath::File(PathBuf::from(OsStr::from_bytes(p.to_bytes())))
        } else if let Some(s) = self.s {
            DbPath::Shm(s.to_owned())
        } else if let Some(spec) = self.m {
            let (name, opt_size) = parse_mmap_spec(spec)?;
            if let Some(sz) = opt_size {
                if sz < 100 {
                    return Err(l_builtin_usage_error!(
                    b"shm: -M NAME:SIZE requires SIZE >= 100 bytes"
                ));
                }
            }
            DbPath::Mmap {
                name,
                size: opt_size,
            }
        } else if let Some(a) = self.n {
            DbPath::Mem(a.to_owned())
        } else {
            DbPath::Mem(CString::new("DEFAULT").unwrap())
        };
        if let DbPath::Shm(n) | DbPath::Mem(n) = &loc {
            if !is_valid_var_name(n.to_bytes()) {
                return Err(l_builtin_error!(
                    b"shm: NAME must be a valid shell variable name"
                ));
            }
        }
        Ok(loc)
    }
}

/// Parse a `-M` selector: `NAME:SIZE` (create a named anonymous mmap store) or
/// `NAME` (select an existing named store; `SIZE` is supplied only when creating).
fn parse_mmap_spec(spec: &CStr) -> Result<(CString, Option<u64>), c_int> {
    let bytes = spec.to_bytes();
    let (name_bytes, size_opt) = match bytes.iter().position(|&b| b == b':') {
        Some(idx) => {
            let name = &bytes[..idx];
            let size = &bytes[idx + 1..];
            if name.is_empty() || size.is_empty() {
                return Err(l_builtin_usage_error!(
                    b"shm: -M NAME:SIZE has an empty field"
                ));
            }
            let size = match std::str::from_utf8(size)
                .ok()
                .and_then(|s| s.parse::<u64>().ok())
            {
                Some(s) => s,
                None => {
                    return Err(l_builtin_usage_error!(
                        b"shm: -M SIZE is not a valid number"
                    ));
                }
            };
            (name, Some(size))
        }
        None => (bytes, None),
    };
    let name = CString::new(name_bytes).map_err(|_| {
        l_builtin_usage_error!(b"shm: -M NAME contains a NUL byte")
    })?;
    Ok((name, size_opt))
}

impl ShmBindArgs {
    fn post(&self) -> CmdResult {
        self.loc.post()
    }
}

impl ShmUnbindArgs {
    fn post(&self) -> CmdResult {
        Ok(())
    }
}

impl ShmClearArgs {
    fn post(&self) -> CmdResult {
        self.loc.post()
    }
}

/// `L_builtin shm bind [-A] [-s NAME | -n NAME | -M NAME:SIZE | -F PATH] VAR_NAME...`
///
/// Bind bash variable(s) VAR_NAME (indexed, or associative with `-A`) to a shared
/// database. The database is selected by `-s` (POSIX shared memory), `-n`
/// (anonymous in-memory mapping) or `-F` (a regular file); with none of them the
/// default `DEFAULT` in-memory database is used.
unsafe fn shm_bind_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_BIND_CMD.enter();
    let args = ShmBindArgs::parse(list)?;
    let loc = args.loc.resolve_dbloc()?;
    let db = match get_db_loc(&loc) {
        Ok(d) => d,
        // A memfd/mmap database not yet created in this process: create it. POSIX
        // shared memory and file backings are (re)opened by name via get_db_loc.
        Err(_) => match open_db_loc(&loc) {
            Ok(d) => Arc::new(d),
            Err(e) => {
                // Selecting a named mmap store without the creation `SIZE` is a
                // usage error, since the store doesn't exist yet to select.
                if matches!(&loc, DbPath::Mmap { size: None, .. }) {
                    return Err(l_builtin_usage_error!(
                        b"shm: -M NAME requires -M NAME:SIZE to create"
                    ));
                }
                return Err(l_builtin_error!(e));
            }
        },
    };
    if args.vars.is_empty() {
        return Err(l_builtin_usage_error!(b"shm: missing required argument: VAR_NAME..."));
    }
    for var in &args.vars {
        let cname = CStr::from_ptr(var.as_ptr());
        // Preserve any existing db data for this variable: the getter will read
        // it and override any stale local bash value. If the db entry has a
        // mismatched type (e.g. was an assoc but is now requested as indexed),
        // the getter's `as_array()`/`as_assoc()` guard skips it harmlessly.
        REGISTRY.with(|r| r.borrow_mut().insert(cname.to_owned(), db.clone()));
        let result = if args.assoc {
            l_init_dynamic_assoc_var(
                var.as_ptr() as *mut c_char,
                Some(shm_assoc_getter),
                Some(shm_assoc_setter),
            )
        } else {
            l_init_dynamic_array_var(
                var.as_ptr() as *mut c_char,
                Some(shm_array_getter),
                Some(shm_array_setter),
            )
        };
        if result.is_null() {
            return Err(l_builtin_error!(b": failed to bind variable ", var.as_ptr()));
        }
    }
    Ok(())
}

/// `L_builtin shm sync [-s NAME | -n NAME | -M NAME | -F PATH] VAR_NAME`
///
/// Push the current bash variable values into the shared database, replacing the
/// variable's existing entry. Unlike `bind` (which binds a new dynamic variable),
/// `sync` is for variables already bound via `bind` -- it snapshots the current
/// bash array/assoc contents into the shared blob.
unsafe fn shm_sync_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_SYNC_CMD.enter();
    let args = ShmSyncArgs::parse(list)?;
    let loc = args.loc.resolve_dbloc()?;
    let db = get_db_loc(&loc).map_err(|e| l_builtin_error!(e))?;
    for var in &args.vars {
        let cname = CStr::from_ptr(var.as_ptr());
        let name = cname.to_owned();
        let shellvar = unsafe { find_variable(var.as_ptr()) };
        if shellvar.is_null() {
            return Err(l_builtin_error!(b"shm: variable not found: ", var.as_ptr()));
        }

        let is_assoc = unsafe { l_assoc_p(shellvar) } != 0;

        let mut new_data = if is_assoc {
            VarData::Assoc(HashMap::new())
        } else {
            VarData::Array(HashMap::new())
        };

        if is_assoc {
            let hash = unsafe { l_assoc_cell(shellvar as *mut SHELL_VAR) };
            for (k, v) in AssocIterator::new(hash) {
                if let VarData::Assoc(m) = &mut new_data {
                    m.insert(k.to_owned(), v.to_owned());
                }
            }
        } else {
            let arr = unsafe { l_array_cell(shellvar as *mut SHELL_VAR) };
            for (idx, val) in ArrayIterator::new(arr) {
                if let VarData::Array(m) = &mut new_data {
                    m.insert(idx, val.to_owned());
                }
            }
        }

        db.with_write(|repr| {
            repr.vars.insert(name, new_data);
        })
        .map_err(|e| l_builtin_error!(e))?;
    }
    Ok(())
}

/// `L_builtin shm rm [-s NAME | -n NAME | -M NAME | -F PATH]`
///
/// Remove the whole database: unbind every variable this shell has bound to it,
/// drop the registry entries, and unlink the backing object/file (for `-s`/`-F`).
/// Takes no positional arguments; the database is selected by the flags, or the
/// default `DEFAULT` database when none are given.
unsafe fn shm_rm_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_REMOVE_CMD.enter();
    let args = ShmLocArgs::parse(list)?;
    let loc = args.resolve_dbloc()?;
    let key = loc_key(&loc);
    unbind_registry_vars(&key);
    unlink_db_backing(&loc);
    Ok(())
}

/// `L_builtin shm unbind VAR_NAME [VAR_NAME...]`
///
/// Unbind the named variable(s) from this shell: drop the registry entry and
/// unbind the bash variable. This does NOT remove the variable's data from the
/// shared database; another process may still read it.
unsafe fn shm_unbind_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_UNBIND_CMD.enter();
    let args = ShmUnbindArgs::parse(list)?;
    if args.vars.as_ptr().is_null() {
        return Err(l_builtin_usage_error!(
            b"shm: missing required argument: VARS"
        ));
    }
    for c in args.vars {
        let v = c.as_ptr() as *const c_char;
        let cname = CStr::from_ptr(v);
        REGISTRY.with(|r| r.borrow_mut().remove(cname));
        l_unbind_variable(v);
    }
    Ok(())
}

/// `L_builtin shm drop VAR_NAME...`
///
/// Remove each variable's data from its shared database, and drop the local
/// binding in this shell. The store is located via each variable's binding (no
/// backing flags), since each bash variable is bound to exactly one store. Use
/// `rm` to destroy a whole store; `drop` targets one variable.
unsafe fn shm_drop_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_DROP_CMD.enter();
    let args = ShmDropArgs::parse(list)?;
    for var in &args.vars {
        let cname = CStr::from_ptr(var.as_ptr());
        let db = match get_shm(cname) {
            Some(db) => db,
            // `get_shm` already printed a scoped error.
            None => return Err(EXECUTION_FAILURE),
        };
        db.with_write(|repr| {
            repr.vars.remove(cname)
        })
        .map_err(|e| l_builtin_error!(e))?;
        // Drop the local binding (registry entry + bash dynamic var).
        REGISTRY.with(|r| r.borrow_mut().remove(cname));
        unsafe { l_unbind_variable(cname.as_ptr()) };
    }
    Ok(())
}

/// `L_builtin shm clear [-s NAME | -n NAME | -M NAME | -F PATH]`
///
/// Wipe every variable's data from the selected database, leaving the backing
/// object/file in place. Bound bash variables in this shell are left bound (they
/// will read as empty until re-added); use `rm` to also drop the backing.
unsafe fn shm_clear_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_CLEAR_CMD.enter();
    let args = ShmClearArgs::parse(list)?;
    let loc = args.loc.resolve_dbloc()?;
    let db = get_db_loc(&loc).map_err(|e| l_builtin_error!(e))?;
    db.with_write(|repr| {
        repr.vars.clear();
    })
    .map_err(|e| l_builtin_error!(e))?;
    Ok(())
}

/// A human-readable label for a database backing, used as a header in `ls`/
/// `info` output.
fn db_key(path: &DbPath) -> String {
    match path {
        DbPath::File(p) => p.to_string_lossy().into_owned(),
        DbPath::Shm(n) => format!("shm:{}", n.to_string_lossy()),
        DbPath::Mem(n) => format!("memfd:{}", n.to_string_lossy()),
        DbPath::Mmap { name, .. } => format!("mmap:{}", name.to_string_lossy()),
    }
}

/// Print one variable as a bash array assignment (`NAME=( ... )`).
unsafe fn print_var_assignment(name: &CString, vd: &VarData) {
    let mut line: Vec<u8> = Vec::new();
    line.extend_from_slice(name.as_bytes());
    line.extend_from_slice(b"=(");
    let mut first = true;
    match vd {
        VarData::Array(map) => {
            let mut entries: Vec<(&i64, &CString)> = map.iter().collect();
            entries.sort_by_key(|(idx, _)| **idx);
            for (idx, v) in entries {
                if !first {
                    line.push(b' ');
                }
                first = false;
                line.extend_from_slice(format!("[{}]=", idx).as_bytes());
                escape_quoted(&mut line, v.as_bytes());
            }
        }
        VarData::Assoc(map) => {
            for (k, v) in map {
                if !first {
                    line.push(b' ');
                }
                first = false;
                line.extend_from_slice(b"[");
                escape_quoted(&mut line, k.as_bytes());
                line.extend_from_slice(b"]=");
                escape_quoted(&mut line, v.as_bytes());
            }
        }
    }
    line.extend_from_slice(b")");
    bprintln!(line);
}

/// `L_builtin shm info [-s NAME | -n NAME | -M NAME | -F PATH]`
///
/// Print every variable stored in the database as bash array assignments. The
/// database is selected by the flags, or the default `DEFAULT` database when
/// none are given.
unsafe fn shm_info_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_INFO_CMD.enter();
    let args = ShmLocArgs::parse(list)?;
    let loc = args.resolve_dbloc()?;
    let db = get_db_loc(&loc).map_err(|e| l_builtin_error!(e))?;
    let repr = db.read().map_err(|e| l_builtin_error!(e))?;
    for (name, vd) in &repr.vars {
        print_var_assignment(name, vd);
    }
    Ok(())
}

/// `L_builtin shm ls [-s NAME | -n NAME | -M NAME | -F PATH]`
///
/// Without any flag, list every database this session knows about together with
/// the variables bound to each. With a backing flag, list only the variables
/// bound to that database in this session's REGISTRY.
unsafe fn shm_ls_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_LIST_CMD.enter();
    let args = ShmLocArgs::parse(list)?;
    if args.s.is_some() || args.n.is_some() || args.m.is_some() || args.f.is_some() {
        let loc = args.resolve_dbloc()?;
        let key = loc_key(&loc);
        let mut names: Vec<CString> = REGISTRY.with(|r| {
            r.borrow()
                .iter()
                .filter(|(_, db)| db_key(&db.path()) == key)
                .map(|(var, _)| var.clone())
                .collect()
        });
        names.sort_by(|a, b| a.as_bytes().cmp(b.as_bytes()));
        for n in &names {
            bprintln!(n.as_bytes());
        }
        return Ok(());
    }
    REGISTRY.with(|r| {
        let reg = r.borrow();
        let mut groups: HashMap<String, Vec<CString>> = HashMap::new();
        for (var, db) in reg.iter() {
            groups
                .entry(db_key(&db.path()))
                .or_default()
                .push(var.clone());
        }
        let mut keys: Vec<&String> = groups.keys().collect();
        keys.sort();
        for k in keys {
            let mut line: Vec<u8> = Vec::new();
            line.extend_from_slice(b"DB ");
            line.extend_from_slice(k.as_bytes());
            line.extend_from_slice(b": ");
            let mut first = true;
            for v in &groups[k] {
                if !first {
                    line.push(b' ');
                }
                first = false;
                line.extend_from_slice(v.as_bytes());
            }
            bprintln!(line);
        }
    });
    Ok(())
}

const SHM_CMD: CmdDesc = CmdDesc::new(
    c"shm",
    c"bind [-A] [-s NAME | -n NAME | -M NAME | -F PATH] VAR_NAME | rm [-s NAME | -n NAME | -M NAME | -F PATH] | unbind VAR_NAME... | drop VAR_NAME | clear [-s NAME | -n NAME | -M NAME | -F PATH] | info [-s NAME | -n NAME | -M NAME | -F PATH] | ls [-s NAME | -n NAME | -M NAME | -F PATH] | sync [-s NAME | -n NAME | -M NAME | -F PATH] VAR_NAME",
    c"\
Shared-memory variables backed by a rkyv database.

Each backing is referenced by a single, consistent selector:
  -s NAME   a POSIX shared memory object (shm_open), shared across processes;
  -n NAME   an anonymous in-memory mapping (memfd_create), shared with forked children;
  -M NAME   a fixed-size anonymous mmap (MAP_SHARED|MAP_ANONYMOUS), shared with
            forked children. Created with `-M NAME:SIZE` (SIZE in bytes, >= 100);
            once named, referenced by NAME alone. Write overflow fails with exit 1.
  -F PATH   a regular file, shared across processes;
  (none)    the default in-memory mapping named DEFAULT (same as -n DEFAULT).

Subcommands:
  bind [-A] [-s NAME | -n NAME | -M NAME:SIZE | -F PATH] VAR_NAME
                            Bind bash variable VAR_NAME (indexed, or associative
                            with -A) to a shared database and store its value.
                            -s/-n/-M/-F selects the backing (see above); none uses
                            DEFAULT. The value is stored under VAR_NAME.
  rm [-s NAME | -n NAME | -M NAME | -F PATH]
                            Destroy the whole database: unbind every variable bound
                            to it and unlink/drop its backing object/file (-s/-F).
                            Bound variables become empty; the store is gone.
  unbind VAR_NAME...
                            Drop the local binding(s) only; store data is untouched,
                            and other processes may still read it. The store is found
                            via each variable's existing binding.
  drop VAR_NAME
                            Erase VAR's data from its store and drop the local
                            binding. The store is found via VAR's binding; use `rm`
                            to destroy a whole store.
  clear [-s NAME | -n NAME | -M NAME | -F PATH]
                            Wipe all variables' data from the selected store,
                            leaving the backing in place. Bound vars stay bound
                            (read as empty).
  info [-s NAME | -n NAME | -M NAME | -F PATH]
                            Print every variable stored in the database (default:
                            the DEFAULT database).
  ls  [-s NAME | -n NAME | -M NAME | -F PATH]
                            List databases. With no flag, list every database this
                            session knows about with the variables bound to each;
                            with a backing flag, list only the variables bound to
                            that database in this session's REGISTRY.
  sync [-s NAME | -n NAME | -M NAME | -F PATH] VAR_NAME
                            Push the current bash variable values into the shared
                            database, replacing the variable's existing entry. The
                            variable must already be bound via 'bind'.

The variable (indexed or associative array) is serialized into a rkyv blob on
every assignment and is visible to every process that maps the same database
 (e.g. a background job started with &, when using -s or -F or -n).
",
);

const SHM_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("bind", shm_bind_subcommand),
    ("rm", shm_rm_subcommand),
    ("unbind", shm_unbind_subcommand),
    ("drop", shm_drop_subcommand),
    ("clear", shm_clear_subcommand),
    ("info", shm_info_subcommand),
    ("ls", shm_ls_subcommand),
    ("sync", shm_sync_subcommand),
];

const SHM_BIND_CMD: CmdDesc = CmdDesc::new(
    c"bind",
    c"bind [-A] [-s NAME | -n NAME | -M NAME:SIZE | -F PATH] VAR_NAME",
    c"\
Bind bash variable VAR_NAME (indexed, or associative with -A) to a shared
database.

The database is selected by one of:
  -s NAME   a POSIX shared memory object (shm_open) named NAME, shared across
            processes;
  -n NAME   an anonymous in-memory mapping (memfd_create) named NAME, shared
            with forked children (the same name within a process tree reuses the
            same mapping);
  -M NAME:SIZE  a fixed-size anonymous mmap (MAP_SHARED|MAP_ANONYMOUS) of SIZE
            bytes, named NAME. Shared with forked children only; the region is
            preallocated and write operations fail when it is exhausted (exit
            status 1). Once created, reference it by NAME alone (-M NAME) from
            any other `shm` subcommand.
  -F PATH   a regular file at PATH (a path on a disc), shared across processes;
  (none)    the default in-memory mapping named DEFAULT (same as -n DEFAULT).
Every assignment is written through to the blob and is visible to every process
that maps the same database, e.g. a background job started with & (for -s/-F/-n).

With -A, create an associative array (key-value pairs with string keys) instead
of an indexed array (integer indices). NAME (for -s/-n/-M) must be a valid shell
variable name; -F takes a path; the SIZE in -M NAME:SIZE must be >= 100 bytes.

Examples:
  L_builtin shm bind v
  v=(a b c)          # default in-memory mapping 'DEFAULT', shared with forked children
  v[0]=changed       # a single-index write is visible to other processes

  L_builtin shm bind -s mydb v
  v=(a b c)          # POSIX shared memory 'mydb', shared across processes

  L_builtin shm bind -F /tmp/mydb v
  v=(a b c)          # regular file at /tmp/mydb

  L_builtin shm bind -M store:1048576 v
  v=(a b c)          # named fixed-size anonymous mmap; later cmds use -M store
  v=(big...)         # writes fail (exit 1) if the region is exhausted

  L_builtin shm bind -A -s mydb v
  v=( [foo]=bar [baz]=qux )  # associative array in shared memory 'mydb'
",
);

const SHM_REMOVE_CMD: CmdDesc = CmdDesc::new(
    c"rm",
    c"rm [-s NAME | -n NAME | -M NAME | -F PATH]",
    c"\
Remove the whole shared database: unbind every variable this shell has bound to
it, drop the registry entries, and unlink the backing object/file (for -s/-F).

The database is selected by the same -s/-n/-F flags as 'bind'; with none given,
the default 'DEFAULT' database is removed.

Examples:
  L_builtin shm rm -s mydb   # remove shared memory 'mydb' entirely
  L_builtin shm rm -n mymem  # remove the in-memory mapping 'mymem'
  L_builtin shm rm          # remove the default 'DEFAULT' database
",
);

const SHM_UNBIND_CMD: CmdDesc = CmdDesc::new(
    c"unbind",
    c"unbind VAR_NAME [VAR_NAME...]",
    c"\
Unbind the named variable(s) from this shell: drop the registry entry and unbind
the bash variable. This does NOT remove the variable's data from the shared
database; another process that has the variable bound may still read it.

The store is found via each variable's existing binding; no backing flags are
needed.

Examples:
  L_builtin shm unbind v         # stop sharing 'v' in this shell
  L_builtin shm unbind v w       # unbind 'v' and 'w'
",
);

const SHM_DROP_CMD: CmdDesc = CmdDesc::new(
    c"drop",
    c"drop VAR_NAME",
    c"\
Remove a single variable's data from its shared database and drop the local
binding in this shell. The store is located via the variable's binding (each
bash variable is bound to exactly one store), so no backing flags are needed.

This is the data-deleting counterpart to `unbind` (which only drops the local
binding): `drop` also erases the variable's entry from the shared blob, so other
processes mapping the same store no longer see it. To destroy a whole store, use
`rm`.

Examples:
  L_builtin shm drop v           # erase v's data and unbind it in this shell
",
);

const SHM_CLEAR_CMD: CmdDesc = CmdDesc::new(
    c"clear",
    c"clear [-s NAME | -n NAME | -M NAME | -F PATH]",
    c"\
Wipe every variable's data from the selected database, leaving the backing
object/file in place. Variables bound in this shell are left bound (they read as
empty until re-added); use `rm` to also drop the backing.

The database is selected by the backing flags, or the default `DEFAULT`
database when none are given.

Examples:
  L_builtin shm clear -s mydb     # empty shared mem 'mydb', keep the object
  L_builtin shm clear             # empty the default 'DEFAULT' database
",
);

const SHM_INFO_CMD: CmdDesc = CmdDesc::new(
    c"info",
    c"info [-s NAME | -n NAME | -M NAME | -F PATH]",
    c"\
Print every variable stored in a shared-memory database.

The database is selected by the same -s/-n/-F flags as 'bind' (default: the
'DEFAULT' database). The output is a series of bash array assignments, one per
variable, that can be eval'd to reconstruct the shared state.

Examples:
  L_builtin shm info -s mydb
",
);

const SHM_LIST_CMD: CmdDesc = CmdDesc::new(
    c"ls",
    c"ls [-s NAME | -n NAME | -M NAME | -F PATH]",
    c"\
List databases. With no flag, list every database this shell session currently
knows about, together with the bash variables bound to each. With a backing flag,
list only the variables bound to that database in this session's REGISTRY.

Databases are shown by their backing kind and name: 'shm:NAME' for POSIX shared
memory, 'memfd:NAME' for in-memory, and the file path for -F databases.
",
);

const SHM_SYNC_CMD: CmdDesc = CmdDesc::new(
    c"sync",
    c"sync [-s NAME | -n NAME | -M NAME | -F PATH] VAR_NAME",
    c"\
Push the current bash variable values into the shared database, replacing the
variable's existing entry. The variable must already be bound to the database
the database, via 'bind'. For each element in the bash array (indexed or associative), the
current value is written to the database.

Normally the dynamic setter (invoked on each element assignment) keeps the
database in sync automatically. However, a bulk array reassignment such as
v=( new1 new2 new3 ) only triggers the setter for the new elements -- stale
elements from the previous array are not removed from the database. 'sync'
is useful for propagating structural changes (element deletion, array
shrinking) or for explicitly committing the current state after a batch of
operations.

Examples:
   L_builtin shm bind -s mydb v
   v=( a b c )
   L_builtin shm sync -s mydb v       # push v=(a b c) into shared mem 'mydb'
",
);

const SHM_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 8> =
    crate::intlookup!(&SHM_SUBCOMMANDS);

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn shm_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_CMD.enter();
    let args = SubCommandCallerArgs::parse(list)?;
    let caller = args.handler(SHM_TABLE)?;
    caller.call()
}
