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
//! `-n NAME` (anonymous in-memory mapping), `-f PATH` (regular file); with none
//! of them the default in-memory mapping named `DEFAULT` is used.
//!
//! Interface:
//!   L_builtin shm add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME
//!       Bind bash variable VAR_NAME (indexed, or associative with -A) to a
//!       shared database.
//!   L_builtin shm rm [-s NAME | -n NAME | -f PATH]
//!       Remove the whole database (unbind its variables, unlink backing).
//!   L_builtin shm unbind [-s NAME | -n NAME | -f PATH] VAR_NAME [VAR_NAME...]
//!       Unbind variable(s) from this shell (drop registry entry); the data
//!       stays in the database.
//!   L_builtin shm info [-s NAME | -n NAME | -f PATH]
//!       Print every variable stored in the database.
//!   L_builtin shm ls [-s NAME | -n NAME | -f PATH]
//!       List databases and the variables bound to each.
//!
//! Example (indexed array, default database):
//!   L_builtin shm add MYVAR
//!   ( sleep 1; echo "${MYVAR[@]}" ) &
//!   MYVAR=( a b c )
//!   wait
//!   # the background job prints "a b c"
//!
//! Example (associative array, POSIX shared memory):
//!   L_builtin shm add -A -s MYSHM MYVAR
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
    array_insert, array_remove, arrayind_t, assoc_remove, is_valid_var_name, l_array_cell,
    l_array_max_index, l_assoc_cell, l_assoc_insert, l_init_dynamic_array_var,
    l_init_dynamic_assoc_var, l_unbind_variable, variable, ArrayIterator, AssocIterator,
    WordListIterCpnt, EXECUTION_FAILURE, EX_USAGE, SHELL_VAR, WORD_LIST,
};
use crate::subcmd::{CmdDesc, CmdResult, SubcommandFn};
use crate::vardb::{open_db_loc, DbLoc, DbPath, LockedDatabase, VarData};
use crate::{beprintln, bprintln, l_builtin_error};

/// Bash variable attribute for associative arrays (att). From bash's
/// variables.h: #define att_assoc 0x0000040
const ATT_ASSOC: c_int = 0x0000040;

thread_local! {
    /// Registry: bash variable name -> opened shared database for its name.
    static REGISTRY: std::cell::RefCell<HashMap<CString, Arc<LockedDatabase>>> =
        std::cell::RefCell::new(HashMap::new());
}

/// Find (or open) the existing database for `loc`: file-backed and POSIX-shm
/// databases are reopened by name/path, while an in-memory `Mem` database can
/// only be shared with forked children and so must already be bound (it is found
/// through the registry).
fn get_db_loc(loc: &DbLoc) -> Result<Arc<LockedDatabase>, String> {
    match loc {
        DbLoc::Mem(name) => REGISTRY.with(|r| {
            r.borrow()
                .values()
                .find(|db| matches!(&db.path, DbPath::Mem(n) if n.as_bytes() == name.to_bytes()))
                .cloned()
                .ok_or_else(|| {
                    format!(
                        "shm: no anonymous database named {}",
                        name.to_str().unwrap_or("")
                    )
                })
        }),
        DbLoc::Shm(_) | DbLoc::File(_) => open_db_loc(loc).map(Arc::new),
    }
}

/// A stable key for a database location, matching the keys produced by
/// [`db_key`] for an opened database.
fn loc_key(loc: &DbLoc) -> String {
    let path = match loc {
        DbLoc::File(p) => DbPath::File(p.clone()),
        DbLoc::Shm(n) => DbPath::Shm(n.clone()),
        DbLoc::Mem(n) => DbPath::Mem(n.clone()),
    };
    db_key(&path)
}

/// Unlink the backing object/file for a database location. An in-memory (memfd)
/// database has no backing object to unlink; its data disappears when the last
/// reference is dropped.
fn unlink_db_backing(loc: &DbLoc) {
    match loc {
        DbLoc::File(p) => {
            let _ = std::fs::remove_file(p);
        }
        DbLoc::Shm(name) => LockedDatabase::unlink_shm(name),
        DbLoc::Mem(_) => {}
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
            if db_key(&db.path) == key {
                to_remove.push(var.clone());
            }
        }
    });
    for var in to_remove {
        unsafe { l_unbind_variable(var.as_ptr() as *const c_char) };
        REGISTRY.with(|r| {
            r.borrow_mut().remove(&var);
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
    let db = REGISTRY.with(|r| r.borrow().get(var).cloned());
    if db.is_none() {
        beprintln!(
            b"L_builtin: shm: variable ",
            var.to_bytes(),
            b" is not bound to a shared database; bind it with `L_builtin shm add \
<NAME>`, or it was unbound/removed (shm unbind / shm rm) while the shell variable still exists",
        );
    }
    db
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
    ($($arg:tt)*) => {
        if true {
            bprintln!($($arg)*);
        }
    };
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
    let name = CString::new(var_name.to_bytes()).unwrap_or_default();
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
    let name = CString::new(var_name.to_bytes()).unwrap_or_default();
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

/// Shared backing flags (`-s`/`-n`/`-f`) for the `shm` subcommands that only
/// select a database. `post()` enforces that at most one of them is given.
#[derive(CmdArgs)]
struct ShmLocArgs {
    /// POSIX shared memory object name (`-s NAME`).
    #[opt('s')]
    s: Option<&'static CStr>,
    /// Anonymous in-memory mapping name (`-n NAME`).
    #[opt('n')]
    n: Option<&'static CStr>,
    /// Regular file path (`-f PATH`).
    #[opt('f')]
    f: Option<&'static CStr>,
}

/// `shm add` arguments: the shared backing flags plus `-A` (associative) and the
/// required `VAR_NAME` positional.
#[derive(CmdArgs)]
struct ShmAddArgs {
    /// Backing selection flags, shared with the other `shm` subcommands.
    #[flatten]
    loc: ShmLocArgs,
    /// Create an associative array instead of an indexed array.
    #[flag('A')]
    assoc: bool,
    /// Bash variable name to bind.
    #[positional]
    var: *const c_char,
}

/// `shm unbind` arguments: the shared backing flags plus one or more `VAR_NAME`s.
#[derive(CmdArgs)]
struct ShmUnbindArgs {
    /// Backing selection flags, shared with the other `shm` subcommands.
    #[flatten]
    loc: ShmLocArgs,
    /// Bash variable names to unbind (one or more).
    #[rest]
    vars: WordListIterCpnt<'static>,
}

/// Validate that at most one of `-s`, `-n`, `-f` was supplied. Shared by every
/// `shm` args struct via its inherent `post` implementation.
impl ShmLocArgs {
    fn post(&self) -> CmdResult {
        let n = [self.s, self.n, self.f]
            .iter()
            .filter(|o| o.is_some())
            .count();
        if n > 1 {
            l_builtin_error!(b"shm: -s, -n and -f are mutually exclusive");
            return Err(EX_USAGE);
        }
        Ok(())
    }

    /// Resolve the backing database location for this parsed set of flags, after
    /// `post()` has confirmed they are mutually exclusive. Also rejects an invalid
    /// `NAME` for the `-s`/`-n` backings.
    fn resolve_dbloc(&self) -> Result<DbLoc, c_int> {
        let loc = if let Some(p) = self.f {
            DbLoc::File(PathBuf::from(OsStr::from_bytes(p.to_bytes())))
        } else if let Some(s) = self.s {
            DbLoc::Shm(s.to_owned())
        } else if let Some(a) = self.n {
            DbLoc::Mem(a.to_owned())
        } else {
            DbLoc::Mem(CString::new("DEFAULT").unwrap())
        };
        if let DbLoc::Shm(n) | DbLoc::Mem(n) = &loc {
            if !is_valid_var_name(n.to_bytes()) {
                l_builtin_error!(b"shm: NAME must be a valid shell variable name");
                return Result::Err(EXECUTION_FAILURE);
            }
        }
        Result::Ok(loc)
    }
}

impl ShmAddArgs {
    fn post(&self) -> CmdResult {
        self.loc.post()
    }
}

impl ShmUnbindArgs {
    fn post(&self) -> CmdResult {
        self.loc.post()
    }
}

/// Resolve the database location from the three optional backing identifiers.
/// Mutual exclusivity of `-s`/`-n`/`-f` is enforced earlier by [`ShmLocArgs::post`].
fn resolve_loc(shared: Option<&CStr>, full: Option<&CStr>, anon: Option<&CStr>) -> DbLoc {
    if let Some(p) = full {
        DbLoc::File(PathBuf::from(OsStr::from_bytes(p.to_bytes())))
    } else if let Some(s) = shared {
        DbLoc::Shm(s.to_owned())
    } else if let Some(a) = anon {
        DbLoc::Mem(a.to_owned())
    } else {
        DbLoc::Mem(CString::new("DEFAULT").unwrap())
    }
}

/// `L_builtin shm add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME`
///
/// Bind bash variable VAR_NAME (indexed, or associative with `-A`) to a shared
/// database. The database is selected by `-s` (POSIX shared memory), `-n`
/// (anonymous in-memory mapping) or `-f` (a regular file); with none of them the
/// default `DEFAULT` in-memory database is used.
unsafe fn shm_add_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_ADD_CMD.enter();
    let args = ShmAddArgs::parse(list)?;
    let loc = args.loc.resolve_dbloc()?;
    let db = match get_db_loc(&loc) {
        Ok(d) => d,
        // A memfd database not yet created in this process: create it. POSIX
        // shared memory and file backings are (re)opened by name via get_db_loc.
        Err(_) => match open_db_loc(&loc) {
            Ok(d) => Arc::new(d),
            Err(e) => {
                l_builtin_error!(e);
                return Err(EXECUTION_FAILURE);
            }
        },
    };
    let cname = CStr::from_ptr(args.var);
    // Start the variable fresh: drop any previous binding of the same name in
    // the shared database (which may have had the opposite type).
    let _ = db.with_write(|repr| {
        repr.vars.remove(cname);
    });
    REGISTRY.with(|r| r.borrow_mut().insert(cname.to_owned(), db));
    let result = if args.assoc {
        l_init_dynamic_assoc_var(
            args.var as *mut c_char,
            Some(shm_assoc_getter),
            Some(shm_assoc_setter),
            ATT_ASSOC,
        )
    } else {
        l_init_dynamic_array_var(
            args.var as *mut c_char,
            Some(shm_array_getter),
            Some(shm_array_setter),
            0,
        )
    };
    if result.is_null() {
        l_builtin_error!(b": failed to bind variable ", args.var);
        return Err(EXECUTION_FAILURE);
    }
    Ok(())
}

/// `L_builtin shm rm [-s NAME | -n NAME | -f PATH]`
///
/// Remove the whole database: unbind every variable this shell has bound to it,
/// drop the registry entries, and unlink the backing object/file (for `-s`/`-f`).
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

/// `L_builtin shm unbind [-s NAME | -n NAME | -f PATH] VAR_NAME [VAR_NAME...]`
///
/// Unbind the named variable(s) from this shell: drop the registry entry and
/// unbind the bash variable. This does NOT remove the variable's data from the
/// shared database; another process may still read it.
unsafe fn shm_unbind_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_UNBIND_CMD.enter();
    let args = ShmUnbindArgs::parse(list)?;
    if args.vars.as_ptr().is_null() {
        l_builtin_error!(b"shm: missing required argument: VARS");
        return Err(EX_USAGE);
    }
    for c in args.vars {
        let v = c.as_ptr() as *const c_char;
        let ckey = match CString::new(unsafe { CStr::from_ptr(v).to_bytes() }) {
            Ok(c) => c,
            Err(_) => continue,
        };
        REGISTRY.with(|r| r.borrow_mut().remove(&ckey));
        l_unbind_variable(v);
    }
    Ok(())
}

/// A human-readable label for a database backing, used as a header in `ls`/
/// `info` output.
fn db_key(path: &DbPath) -> String {
    match path {
        DbPath::File(p) => p.to_string_lossy().into_owned(),
        DbPath::Shm(n) => format!("shm:{}", n.to_string_lossy()),
        DbPath::Mem(n) => format!("memfd:{}", n.to_string_lossy()),
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

/// `L_builtin shm info [-s NAME | -n NAME | -f PATH]`
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

/// `L_builtin shm ls [-s NAME | -n NAME | -f PATH]`
///
/// Without any flag, list every database this session knows about together with
/// the variables bound to each. With a backing flag, list only the variables
/// bound to that database in this session's REGISTRY.
unsafe fn shm_ls_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_LIST_CMD.enter();
    let args = ShmLocArgs::parse(list)?;
    if args.s.is_some() || args.n.is_some() || args.f.is_some() {
        let loc = args.resolve_dbloc()?;
        // List only the variables bound to the database in this session's
        // REGISTRY (not every entry another process may have written to it).
        let key = loc_key(&loc);
        let mut names: Vec<CString> = REGISTRY.with(|r| {
            r.borrow()
                .iter()
                .filter(|(_, db)| db_key(&db.path) == key)
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
                .entry(db_key(&db.path))
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
    c"add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME | rm [-s NAME | -n NAME | -f PATH] | unbind [-s NAME | -n NAME | -f PATH] VAR_NAME... | info [-s NAME | -n NAME | -f PATH] | ls [-s NAME | -n NAME | -f PATH]",
    c"\
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

The variable (indexed or associative array) is serialized into a rkyv blob on
every assignment and is visible to every process that maps the same database
 (e.g. a background job started with &, when using -s or -f or -n).
",
);

const SHM_SUBCOMMANDS: &[(&str, SubcommandFn)] = &[
    ("add", shm_add_subcommand),
    ("rm", shm_rm_subcommand),
    ("unbind", shm_unbind_subcommand),
    ("info", shm_info_subcommand),
    ("ls", shm_ls_subcommand),
];

const SHM_ADD_CMD: CmdDesc = CmdDesc::new(
    c"add",
    c"add [-A] [-s NAME | -n NAME | -f PATH] VAR_NAME",
    c"\
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
",
);

const SHM_REMOVE_CMD: CmdDesc = CmdDesc::new(
    c"rm",
    c"rm [-s NAME | -n NAME | -f PATH]",
    c"\
Remove the whole shared database: unbind every variable this shell has bound to
it, drop the registry entries, and unlink the backing object/file (for -s/-f).

The database is selected by the same -s/-n/-f flags as 'add'; with none given,
the default 'DEFAULT' database is removed.

Examples:
  L_builtin shm rm -s mydb   # remove shared memory 'mydb' entirely
  L_builtin shm rm -n mymem  # remove the in-memory mapping 'mymem'
  L_builtin shm rm          # remove the default 'DEFAULT' database
",
);

const SHM_UNBIND_CMD: CmdDesc = CmdDesc::new(
    c"unbind",
    c"unbind [-s NAME | -n NAME | -f PATH] VAR_NAME [VAR_NAME...]",
    c"\
Unbind the named variable(s) from this shell: drop the registry entry and unbind
the bash variable. This does NOT remove the variable's data from the shared
database; another process that has the variable bound may still read it.

The database is selected by the same -s/-n/-f flags as 'add'; with none given,
the default 'DEFAULT' database is used.

Examples:
  L_builtin shm unbind -s mydb v   # stop sharing 'v' from shared memory 'mydb'
  L_builtin shm unbind v w         # unbind 'v' and 'w' from the default database
",
);

const SHM_INFO_CMD: CmdDesc = CmdDesc::new(
    c"info",
    c"info [-s NAME | -n NAME | -f PATH]",
    c"\
Print every variable stored in a shared-memory database.

The database is selected by the same -s/-n/-f flags as 'add' (default: the
'DEFAULT' database). The output is a series of bash array assignments, one per
variable, that can be eval'd to reconstruct the shared state.

Examples:
  L_builtin shm info -s mydb
",
);

const SHM_LIST_CMD: CmdDesc = CmdDesc::new(
    c"ls",
    c"ls [-s NAME | -n NAME | -f PATH]",
    c"\
List databases. With no flag, list every database this shell session currently
knows about, together with the bash variables bound to each. With a backing flag,
list only the variables bound to that database in this session's REGISTRY.

Databases are shown by their backing kind and name: 'shm:NAME' for POSIX shared
memory, 'memfd:NAME' for in-memory, and the file path for -f databases.
",
);

const SHM_TABLE: crate::intlookup::U64::IntLookup<SubcommandFn, 5> =
    crate::intlookup!(&SHM_SUBCOMMANDS);

#[derive(CmdArgs)]
struct ShmDispatchArgs {
    #[positional]
    action: *const c_char,
    #[rest]
    rest: WordListIterCpnt<'static>,
}

/// # Safety
///
/// Safe when called from bash with a valid WORD_LIST pointer.
pub unsafe fn shm_subcommand(list: *mut WORD_LIST) -> CmdResult {
    SHM_CMD.enter();
    let args = ShmDispatchArgs::parse(list)?;
    let action_bytes = unsafe { CStr::from_ptr(args.action) }.to_bytes();
    let handler = match SHM_TABLE.lookup(action_bytes) {
        Some(h) => h,
        None => {
            l_builtin_error!(b": unknown shm subcommand: ", action_bytes);
            return Err(EXECUTION_FAILURE);
        }
    };
    handler(args.rest.as_ptr())
}
