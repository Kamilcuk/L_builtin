#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]

use std::ffi::CStr;
use std::io::{Cursor, Read};
use std::os::raw::{c_char, c_int, c_uint, c_void};

use tar::Archive;
use zstd::stream::Decoder;

///////////////////////////////////////////////////////////////////////////////

#[repr(C)]
pub struct Builtin {
    pub name: *const c_char,
    pub function: unsafe extern "C" fn(*mut c_void) -> c_int,
    pub flags: c_int,
    pub long_doc: *const *const c_char,
    pub short_doc: *const c_char,
    pub handle: *mut c_char,
}

const BUILTIN_ENABLED: c_int = 1;

#[repr(transparent)]
struct SyncPtr(*const c_char);
unsafe impl Sync for SyncPtr {}

static L_BUILTIN_DOC: [SyncPtr; 7] = [
    SyncPtr(c"L_builtin multi-version dispatcher.".as_ptr()),
    SyncPtr(c"".as_ptr()),
    SyncPtr(c"L_builtin <subcommand> [options] [args]".as_ptr()),
    SyncPtr(c"".as_ptr()),
    SyncPtr(c"Available subcommands:".as_ptr()),
    SyncPtr(c"  version      Print build and bash version information".as_ptr()),
    SyncPtr(core::ptr::null()),
];

#[no_mangle]
pub static mut L_builtin_struct: Builtin = Builtin {
    name: c"L_builtin".as_ptr(),
    function: l_entrypoint,
    flags: BUILTIN_ENABLED,
    long_doc: L_BUILTIN_DOC.as_ptr() as *const *const c_char,
    short_doc: c"L_builtin [-v <var>] <subcommand> [options] [args...]".as_ptr(),
    handle: core::ptr::null_mut(),
};

///////////////////////////////////////////////////////////////////////////////

extern "C" {
    static dist_version: *const c_char;

    fn dlopen(filename: *const c_char, flag: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlerror() -> *mut c_char;
    fn memfd_create(name: *const c_char, flags: c_uint) -> c_int;
    fn write(fd: c_int, buf: *const c_void, count: usize) -> isize;
}

const RTLD_NOW: c_int = libc::RTLD_NOW;
const RTLD_LOCAL: c_int = libc::RTLD_LOCAL;

///////////////////////////////////////////////////////////////////////////////

const EMBEDDED_TAR_ZST: &[u8] = include_bytes!(env!("EMBEDDED_TAR_ZST_PATH"));

fn version_from_string(s: &CStr) -> Option<&str> {
    let str = std::str::from_utf8(s.to_bytes()).ok()?;
    let major_dot = str.find('.')?;
    let rest = &str[major_dot + 1..];
    let minor_end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    Some(&str[..major_dot + 1 + minor_end])
}

fn sh(cmd: &str) {
    eprintln!("+ {}", cmd);
    if let Ok(cstr) = std::ffi::CString::new(cmd) {
        let _ = unsafe { libc::system(cstr.as_ptr()) };
    }
}

fn write_all_fd(fd: c_int, data: &[u8]) -> std::io::Result<()> {
    let mut pos = 0;
    while pos < data.len() {
        let ret = unsafe {
            write(fd, data[pos..].as_ptr() as *const c_void, data.len() - pos)
        };
        if ret < 0 {
            return Err(std::io::Error::last_os_error());
        }
        pos += ret as usize;
    }
    Ok(())
}

fn load_and_decompress_embedded_so(version: &str) -> Option<*mut c_void> {
    let decoder = Decoder::new(Cursor::new(EMBEDDED_TAR_ZST))
        .map_err(|e| eprintln!("L_builtin: zstd decode error: {}", e))
        .ok()?;

    let mut archive = Archive::new(decoder);
    let mut so_data: Option<Vec<u8>> = None;

    for entry in archive.entries().ok()? {
        let mut entry = entry.ok()?;
        let path = entry.path().ok()?;
        let entry_name = path.file_name()?.to_str()?;
        if entry_name == version {
            let mut data = Vec::new();
            Read::read_to_end(&mut entry, &mut data).ok()?;
            so_data = Some(data);
            break;
        }
    }

    drop(archive);
    let so_data = so_data?;

    let fd = unsafe { memfd_create(c"L_builtin_embedded".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        eprintln!("L_builtin: memfd_create: {}", std::io::Error::last_os_error());
        return None;
    }

    if write_all_fd(fd, &so_data).is_err() {
        eprintln!("L_builtin: write to memfd: {}", std::io::Error::last_os_error());
        return None;
    }

    sh(&format!("nm -D -g --defined-only /proc/self/fd/{} | grep L_builtin_struct", fd));
    sh(&format!("readelf -Ws /proc/self/fd/{} | grep L_builtin_struct", fd));
    sh(&format!("readelf -Ws --dyn-syms /proc/self/fd/{} | grep L_builtin_struct", fd));

    let fd_path = format!("/proc/self/fd/{}", fd);
    let path_c = std::ffi::CString::new(fd_path.clone()).ok()?;

    let handle = unsafe { dlopen(path_c.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
    if handle.is_null() {
        let err = unsafe { CStr::from_ptr(dlerror()) };
        eprintln!("L_builtin: dlopen({}): {}", fd_path, err.to_string_lossy());
        return None;
    }

    Some(handle)
}

fn get_embedded_builtin(handle: *mut c_void) -> Option<&'static Builtin> {
    let symbol = std::ffi::CString::new("L_builtin_impl").ok()?;
    let ptr = unsafe { dlsym(handle, symbol.as_ptr()) };
    if ptr.is_null() {
        return None;
    }
    Some(unsafe { &*(ptr as *const Builtin) })
}

///////////////////////////////////////////////////////////////////////////////

#[no_mangle]
pub unsafe extern "C" fn l_entrypoint(list: *mut c_void) -> c_int {
    let dist = CStr::from_ptr(dist_version);
    let version = match version_from_string(&dist) {
        Some(v) => v,
        None => {
            eprintln!("L_builtin: could not parse dist_version");
            return 1;
        }
    };

    let handle = match load_and_decompress_embedded_so(&version) {
        Some(h) => h,
        None => {
            eprintln!("L_builtin: no module for bash {}", version);
            return 1;
        }
    };

    let b = match get_embedded_builtin(handle) {
        Some(b) => b,
        None => {
            let err = CStr::from_ptr(dlerror());
            eprintln!(
                "L_builtin: no L_builtin_impl symbol: {}",
                err.to_string_lossy()
            );
            return 1;
        }
    };

    L_builtin_struct.short_doc = b.short_doc;
    L_builtin_struct.long_doc = b.long_doc;
    L_builtin_struct.function = b.function;

    (b.function)(list)
}
