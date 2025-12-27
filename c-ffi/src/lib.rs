//! C Foreign Function Interface for KeystoneDB
//!
//! This crate provides C-compatible bindings for KeystoneDB,
//! enabling integration with C, Python, JavaScript, and other languages.
//!
//! # Status
//! Currently a stub - full FFI implementation planned for future release.

use std::ffi::{CStr, CString};
use std::os::raw::c_char;
use std::ptr;

/// Opaque handle to a KeystoneDB database
pub struct KstoneDb {
    inner: kstone_api::Database,
}

/// Create a new database at the given path.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - The returned pointer must be freed with `kstone_db_close`
#[no_mangle]
pub unsafe extern "C" fn kstone_db_create(path: *const c_char) -> *mut KstoneDb {
    if path.is_null() {
        return ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    match kstone_api::Database::create(path_str) {
        Ok(db) => Box::into_raw(Box::new(KstoneDb { inner: db })),
        Err(_) => ptr::null_mut(),
    }
}

/// Open an existing database at the given path.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - The returned pointer must be freed with `kstone_db_close`
#[no_mangle]
pub unsafe extern "C" fn kstone_db_open(path: *const c_char) -> *mut KstoneDb {
    if path.is_null() {
        return ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    match kstone_api::Database::open(path_str) {
        Ok(db) => Box::into_raw(Box::new(KstoneDb { inner: db })),
        Err(_) => ptr::null_mut(),
    }
}

/// Close and free a database handle.
///
/// # Safety
/// - `db` must be a valid pointer returned by `kstone_db_create` or `kstone_db_open`
/// - `db` must not be used after this call
#[no_mangle]
pub unsafe extern "C" fn kstone_db_close(db: *mut KstoneDb) {
    if !db.is_null() {
        drop(Box::from_raw(db));
    }
}

/// Get the version of the KeystoneDB library.
///
/// # Safety
/// The returned string is statically allocated and must not be freed.
#[no_mangle]
pub extern "C" fn kstone_version() -> *const c_char {
    static VERSION: &[u8] = b"0.1.0\0";
    VERSION.as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        let version = kstone_version();
        assert!(!version.is_null());
        unsafe {
            let version_str = CStr::from_ptr(version).to_str().unwrap();
            assert_eq!(version_str, "0.1.0");
        }
    }
}
