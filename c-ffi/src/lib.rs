use kstone_api::{Database, ItemBuilder, KeystoneValue};
use kstone_core::{Item, Value};
use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::slice;

/// Error codes returned by FFI functions
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KsError {
    /// Operation succeeded
    Ok = 0,
    /// Null pointer argument
    NullPointer = 1,
    /// Invalid UTF-8 string
    InvalidUtf8 = 2,
    /// Invalid argument
    InvalidArgument = 3,
    /// I/O error
    IoError = 4,
    /// Item not found
    NotFound = 5,
    /// Internal error
    Internal = 6,
    /// Corruption detected
    Corruption = 7,
    /// Conditional check failed
    ConditionalCheckFailed = 8,
}

/// Opaque handle to a Database instance
#[repr(C)]
pub struct KsDatabase {
    _private: [u8; 0],
}

/// Opaque handle to an Item
#[repr(C)]
pub struct KsItem {
    _private: [u8; 0],
}

/// Convert Rust Database to opaque pointer
fn db_to_ptr(db: Database) -> *mut KsDatabase {
    Box::into_raw(Box::new(db)) as *mut KsDatabase
}

/// Convert opaque pointer back to Database reference
unsafe fn ptr_to_db(ptr: *mut KsDatabase) -> Option<&'static mut Database> {
    if ptr.is_null() {
        None
    } else {
        Some(&mut *(ptr as *mut Database))
    }
}

/// Convert Item to opaque pointer
fn item_to_ptr(item: Item) -> *mut KsItem {
    Box::into_raw(Box::new(item)) as *mut KsItem
}

/// Convert opaque pointer back to Item reference
unsafe fn ptr_to_item(ptr: *const KsItem) -> Option<&'static Item> {
    if ptr.is_null() {
        None
    } else {
        Some(&*(ptr as *const Item))
    }
}

/// Thread-local storage for last error message
thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<CString>> = std::cell::RefCell::new(None);
}

/// Set the last error message
fn set_last_error(err: String) {
    LAST_ERROR.with(|e| {
        *e.borrow_mut() = CString::new(err).ok();
    });
}

/// Create a new database at the specified path
///
/// # Arguments
/// * `path` - Path to database directory (null-terminated C string)
/// * `out` - Output pointer to receive database handle
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// Caller must ensure path is a valid null-terminated string
#[no_mangle]
pub unsafe extern "C" fn ks_database_create(
    path: *const c_char,
    out: *mut *mut KsDatabase,
) -> KsError {
    if path.is_null() || out.is_null() {
        return KsError::NullPointer;
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in path".to_string());
            return KsError::InvalidUtf8;
        }
    };

    match Database::create(path_str) {
        Ok(db) => {
            *out = db_to_ptr(db);
            KsError::Ok
        }
        Err(e) => {
            set_last_error(format!("Failed to create database: {}", e));
            KsError::Internal
        }
    }
}

/// Open an existing database at the specified path
///
/// # Arguments
/// * `path` - Path to database directory (null-terminated C string)
/// * `out` - Output pointer to receive database handle
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// Caller must ensure path is a valid null-terminated string
#[no_mangle]
pub unsafe extern "C" fn ks_database_open(
    path: *const c_char,
    out: *mut *mut KsDatabase,
) -> KsError {
    if path.is_null() || out.is_null() {
        return KsError::NullPointer;
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in path".to_string());
            return KsError::InvalidUtf8;
        }
    };

    match Database::open(path_str) {
        Ok(db) => {
            *out = db_to_ptr(db);
            KsError::Ok
        }
        Err(e) => {
            set_last_error(format!("Failed to open database: {}", e));
            KsError::Internal
        }
    }
}

/// Create a new in-memory database
///
/// # Arguments
/// * `out` - Output pointer to receive database handle
///
/// # Returns
/// Error code (0 = success)
#[no_mangle]
pub unsafe extern "C" fn ks_database_create_in_memory(out: *mut *mut KsDatabase) -> KsError {
    if out.is_null() {
        return KsError::NullPointer;
    }

    match Database::create_in_memory() {
        Ok(db) => {
            *out = db_to_ptr(db);
            KsError::Ok
        }
        Err(e) => {
            set_last_error(format!("Failed to create in-memory database: {}", e));
            KsError::Internal
        }
    }
}

/// Close and free a database handle
///
/// # Arguments
/// * `db` - Database handle to close
///
/// # Safety
/// Caller must ensure db is a valid database handle and is not used after this call
#[no_mangle]
pub unsafe extern "C" fn ks_database_close(db: *mut KsDatabase) {
    if !db.is_null() {
        let _ = Box::from_raw(db as *mut Database);
    }
}

/// Put a string value into the database
///
/// # Arguments
/// * `db` - Database handle
/// * `pk` - Partition key (null-terminated C string)
/// * `sk` - Sort key (null-terminated C string, or NULL for no sort key)
/// * `attr_name` - Attribute name (null-terminated C string)
/// * `value` - String value (null-terminated C string)
///
/// # Returns
/// Error code (0 = success)
///
/// # Safety
/// All string pointers must be valid null-terminated C strings
#[no_mangle]
pub unsafe extern "C" fn ks_database_put_string(
    db: *mut KsDatabase,
    pk: *const c_char,
    sk: *const c_char,
    attr_name: *const c_char,
    value: *const c_char,
) -> KsError {
    if db.is_null() || pk.is_null() || attr_name.is_null() || value.is_null() {
        return KsError::NullPointer;
    }

    let database = match ptr_to_db(db) {
        Some(d) => d,
        None => return KsError::NullPointer,
    };

    let pk_bytes = CStr::from_ptr(pk).to_bytes();
    let attr = match CStr::from_ptr(attr_name).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in attribute name".to_string());
            return KsError::InvalidUtf8;
        }
    };
    let val = match CStr::from_ptr(value).to_str() {
        Ok(s) => s,
        Err(_) => {
            set_last_error("Invalid UTF-8 in value".to_string());
            return KsError::InvalidUtf8;
        }
    };

    let mut item = HashMap::new();
    item.insert(attr.to_string(), Value::S(val.to_string()));

    let result = if sk.is_null() {
        database.put(pk_bytes, item)
    } else {
        let sk_bytes = CStr::from_ptr(sk).to_bytes();
        database.put_with_sk(pk_bytes, sk_bytes, item)
    };

    match result {
        Ok(_) => KsError::Ok,
        Err(e) => {
            set_last_error(format!("Put failed: {}", e));
            KsError::Internal
        }
    }
}

/// Get an item from the database
///
/// # Arguments
/// * `db` - Database handle
/// * `pk` - Partition key (null-terminated C string)
/// * `sk` - Sort key (null-terminated C string, or NULL for no sort key)
/// * `out` - Output pointer to receive item handle (NULL if not found)
///
/// # Returns
/// Error code (0 = success, NotFound if item doesn't exist)
///
/// # Safety
/// Caller must free the returned item with ks_item_free
#[no_mangle]
pub unsafe extern "C" fn ks_database_get(
    db: *mut KsDatabase,
    pk: *const c_char,
    sk: *const c_char,
    out: *mut *mut KsItem,
) -> KsError {
    if db.is_null() || pk.is_null() || out.is_null() {
        return KsError::NullPointer;
    }

    let database = match ptr_to_db(db) {
        Some(d) => d,
        None => return KsError::NullPointer,
    };

    let pk_bytes = CStr::from_ptr(pk).to_bytes();

    let result = if sk.is_null() {
        database.get(pk_bytes)
    } else {
        let sk_bytes = CStr::from_ptr(sk).to_bytes();
        database.get_with_sk(pk_bytes, sk_bytes)
    };

    match result {
        Ok(Some(item)) => {
            *out = item_to_ptr(item);
            KsError::Ok
        }
        Ok(None) => {
            *out = ptr::null_mut();
            KsError::NotFound
        }
        Err(e) => {
            set_last_error(format!("Get failed: {}", e));
            *out = ptr::null_mut();
            KsError::Internal
        }
    }
}

/// Delete an item from the database
///
/// # Arguments
/// * `db` - Database handle
/// * `pk` - Partition key (null-terminated C string)
/// * `sk` - Sort key (null-terminated C string, or NULL for no sort key)
///
/// # Returns
/// Error code (0 = success)
#[no_mangle]
pub unsafe extern "C" fn ks_database_delete(
    db: *mut KsDatabase,
    pk: *const c_char,
    sk: *const c_char,
) -> KsError {
    if db.is_null() || pk.is_null() {
        return KsError::NullPointer;
    }

    let database = match ptr_to_db(db) {
        Some(d) => d,
        None => return KsError::NullPointer,
    };

    let pk_bytes = CStr::from_ptr(pk).to_bytes();

    let result = if sk.is_null() {
        database.delete(pk_bytes)
    } else {
        let sk_bytes = CStr::from_ptr(sk).to_bytes();
        database.delete_with_sk(pk_bytes, sk_bytes)
    };

    match result {
        Ok(_) => KsError::Ok,
        Err(e) => {
            set_last_error(format!("Delete failed: {}", e));
            KsError::Internal
        }
    }
}

/// Free an item handle
///
/// # Arguments
/// * `item` - Item handle to free
///
/// # Safety
/// Caller must ensure item is not used after this call
#[no_mangle]
pub unsafe extern "C" fn ks_item_free(item: *mut KsItem) {
    if !item.is_null() {
        let _ = Box::from_raw(item as *mut Item);
    }
}

/// Get the last error message
///
/// # Returns
/// Null-terminated C string with error message, or NULL if no error
///
/// # Safety
/// The returned pointer is valid until the next FFI call on this thread
#[no_mangle]
pub unsafe extern "C" fn ks_get_last_error() -> *const c_char {
    LAST_ERROR.with(|e| {
        e.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(ptr::null())
    })
}
