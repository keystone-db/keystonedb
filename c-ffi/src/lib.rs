//! C Foreign Function Interface for KeystoneDB
//!
//! This crate provides C-compatible bindings for KeystoneDB,
//! enabling integration with C, Python, JavaScript, and other languages.
//!
//! # Memory Management
//! - All objects returned by `kstone_*_create` functions must be freed with corresponding `kstone_*_free` functions
//! - Strings returned by functions are owned and must be freed with `kstone_string_free`
//! - The caller is responsible for freeing all allocated memory
//!
//! # Error Handling
//! - Functions return NULL or error codes on failure
//! - Use `kstone_last_error` to get the last error message
//! - Use `kstone_last_error_code` to get the last error code

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_double, c_int, c_uint};
use std::ptr;

use parking_lot::Mutex;

use kstone_api::{
    BatchGetRequest, BatchWriteRequest, Database, ItemBuilder, Query, Scan, Update,
    KeystoneError, KeystoneValue,
};

// Thread-local error storage
thread_local! {
    static LAST_ERROR: Mutex<Option<(i32, String)>> = Mutex::new(None);
}

/// Error codes
pub const KSTONE_OK: c_int = 0;
pub const KSTONE_ERR_NULL_POINTER: c_int = -1;
pub const KSTONE_ERR_INVALID_UTF8: c_int = -2;
pub const KSTONE_ERR_NOT_FOUND: c_int = -3;
pub const KSTONE_ERR_INVALID_QUERY: c_int = -4;
pub const KSTONE_ERR_INVALID_ARGUMENT: c_int = -5;
pub const KSTONE_ERR_CONDITION_FAILED: c_int = -6;
pub const KSTONE_ERR_TRANSACTION_CANCELED: c_int = -7;
pub const KSTONE_ERR_IO: c_int = -8;
pub const KSTONE_ERR_CORRUPTION: c_int = -9;
pub const KSTONE_ERR_INTERNAL: c_int = -10;

/// Size limits (matching DynamoDB limits)
const MAX_KEY_SIZE: usize = 2048; // 2KB max key size
const MAX_VALUE_SIZE: usize = 400 * 1024; // 400KB max value size

fn set_error(code: i32, message: String) {
    LAST_ERROR.with(|e| {
        *e.lock() = Some((code, message));
    });
}

fn clear_error() {
    LAST_ERROR.with(|e| {
        *e.lock() = None;
    });
}

fn error_from_keystone(err: &KeystoneError) -> i32 {
    match err {
        KeystoneError::NotFound(_) => KSTONE_ERR_NOT_FOUND,
        KeystoneError::InvalidQuery(_) => KSTONE_ERR_INVALID_QUERY,
        KeystoneError::InvalidArgument(_) => KSTONE_ERR_INVALID_ARGUMENT,
        KeystoneError::ConditionalCheckFailed(_) => KSTONE_ERR_CONDITION_FAILED,
        KeystoneError::TransactionCanceled(_) => KSTONE_ERR_TRANSACTION_CANCELED,
        KeystoneError::Io(_) => KSTONE_ERR_IO,
        KeystoneError::Corruption(_) => KSTONE_ERR_CORRUPTION,
        _ => KSTONE_ERR_INTERNAL,
    }
}

/// Validates a database path to prevent path traversal attacks.
/// Returns the validated path or sets an error and returns None.
fn validate_db_path(path_str: &str) -> Option<std::path::PathBuf> {
    use std::path::{Path, Component};

    let path = Path::new(path_str);

    // Check for path traversal attempts
    for component in path.components() {
        if let Component::ParentDir = component {
            set_error(
                KSTONE_ERR_INVALID_ARGUMENT,
                "Path traversal not allowed: '..' in path".to_string(),
            );
            return None;
        }
    }

    Some(path.to_path_buf())
}

/// Validates a key size to prevent unbounded memory allocation.
/// Returns the validated size or sets an error and returns None.
fn validate_key_size(len: c_uint) -> Option<usize> {
    let len = len as usize;
    if len > MAX_KEY_SIZE {
        set_error(
            KSTONE_ERR_INVALID_ARGUMENT,
            format!("Key size {} exceeds maximum {}", len, MAX_KEY_SIZE),
        );
        return None;
    }
    Some(len)
}

/// Validates a value size to prevent unbounded memory allocation.
/// Returns the validated size or sets an error and returns None.
fn validate_value_size(len: c_uint) -> Option<usize> {
    let len = len as usize;
    if len > MAX_VALUE_SIZE {
        set_error(
            KSTONE_ERR_INVALID_ARGUMENT,
            format!("Value size {} exceeds maximum {}", len, MAX_VALUE_SIZE),
        );
        return None;
    }
    Some(len)
}

// ============================================================================
// Opaque Handles
// ============================================================================

/// Opaque handle to a KeystoneDB database
pub struct KstoneDb {
    inner: Database,
}

/// Opaque handle to a KeystoneDB item (HashMap<String, Value>)
pub struct KstoneItem {
    inner: HashMap<String, KeystoneValue>,
}

/// Opaque handle to a KeystoneDB value
pub struct KstoneValue {
    inner: kstone_api::KeystoneValue,
}

/// Opaque handle to a Query builder
pub struct KstoneQuery {
    inner: Query,
}

/// Opaque handle to a Scan builder
pub struct KstoneScan {
    inner: Scan,
}

/// Opaque handle to an Update builder
pub struct KstoneUpdate {
    inner: Update,
}

/// Opaque handle to a BatchGetRequest
pub struct KstoneBatchGet {
    inner: BatchGetRequest,
}

/// Opaque handle to a BatchWriteRequest
pub struct KstoneBatchWrite {
    inner: BatchWriteRequest,
}

/// Opaque handle to an ItemBuilder
pub struct KstoneItemBuilder {
    inner: ItemBuilder,
}

/// Query/Scan response structure
#[repr(C)]
pub struct KstoneQueryResponse {
    pub items: *mut *mut KstoneItem,
    pub item_count: c_uint,
    pub scanned_count: c_uint,
    pub has_more: c_int,
    pub last_pk: *mut c_char,
    pub last_sk: *mut c_char,
}

/// Batch get response structure
#[repr(C)]
pub struct KstoneBatchGetResponse {
    pub items: *mut *mut KstoneItem,
    pub keys: *mut *mut c_char,
    pub item_count: c_uint,
}

// ============================================================================
// Error Handling
// ============================================================================

/// Get the last error message. Returns NULL if no error.
/// The returned string must be freed with `kstone_string_free`.
#[no_mangle]
pub extern "C" fn kstone_last_error() -> *mut c_char {
    LAST_ERROR.with(|e| {
        let guard = e.lock();
        match &*guard {
            Some((_, msg)) => {
                // If CString::new fails (contains null byte), return a safe error message
                match CString::new(msg.as_str()) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        // Error message contains null byte, sanitize it
                        let sanitized = msg.replace('\0', "");
                        CString::new(sanitized).unwrap_or_else(|_| CString::new("Error message contains invalid data").unwrap()).into_raw()
                    }
                }
            }
            None => ptr::null_mut(),
        }
    })
}

/// Get the last error code. Returns KSTONE_OK if no error.
#[no_mangle]
pub extern "C" fn kstone_last_error_code() -> c_int {
    LAST_ERROR.with(|e| {
        let guard = e.lock();
        match &*guard {
            Some((code, _)) => *code,
            None => KSTONE_OK,
        }
    })
}

/// Clear the last error.
#[no_mangle]
pub extern "C" fn kstone_clear_error() {
    clear_error();
}

/// Free a string returned by KeystoneDB functions.
#[no_mangle]
pub unsafe extern "C" fn kstone_string_free(s: *mut c_char) {
    if !s.is_null() {
        drop(CString::from_raw(s));
    }
}

// ============================================================================
// Database Operations
// ============================================================================

/// Create a new database at the given path.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - The returned pointer must be freed with `kstone_db_close`
#[no_mangle]
pub unsafe extern "C" fn kstone_db_create(path: *const c_char) -> *mut KstoneDb {
    clear_error();

    if path.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "path is null".to_string());
        return ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(KSTONE_ERR_INVALID_UTF8, format!("invalid UTF-8 in path: {}", e));
            return ptr::null_mut();
        }
    };

    // Validate path to prevent path traversal attacks
    let validated_path = match validate_db_path(path_str) {
        Some(p) => p,
        None => return ptr::null_mut(), // Error already set by validate_db_path
    };

    match Database::create(&validated_path) {
        Ok(db) => Box::into_raw(Box::new(KstoneDb { inner: db })),
        Err(e) => {
            set_error(error_from_keystone(&e), e.to_string());
            ptr::null_mut()
        }
    }
}

/// Open an existing database at the given path.
///
/// # Safety
/// - `path` must be a valid null-terminated C string
/// - The returned pointer must be freed with `kstone_db_close`
#[no_mangle]
pub unsafe extern "C" fn kstone_db_open(path: *const c_char) -> *mut KstoneDb {
    clear_error();

    if path.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "path is null".to_string());
        return ptr::null_mut();
    }

    let path_str = match CStr::from_ptr(path).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(KSTONE_ERR_INVALID_UTF8, format!("invalid UTF-8 in path: {}", e));
            return ptr::null_mut();
        }
    };

    // Validate path to prevent path traversal attacks
    let validated_path = match validate_db_path(path_str) {
        Some(p) => p,
        None => return ptr::null_mut(), // Error already set by validate_db_path
    };

    match Database::open(&validated_path) {
        Ok(db) => Box::into_raw(Box::new(KstoneDb { inner: db })),
        Err(e) => {
            set_error(error_from_keystone(&e), e.to_string());
            ptr::null_mut()
        }
    }
}

/// Create an in-memory database (no persistence).
///
/// # Safety
/// The returned pointer must be freed with `kstone_db_close`
#[no_mangle]
pub extern "C" fn kstone_db_create_in_memory() -> *mut KstoneDb {
    clear_error();

    match Database::create_in_memory() {
        Ok(db) => Box::into_raw(Box::new(KstoneDb { inner: db })),
        Err(e) => {
            set_error(error_from_keystone(&e), e.to_string());
            ptr::null_mut()
        }
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

/// Flush pending writes to disk.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_flush(db: *mut KstoneDb) -> c_int {
    clear_error();

    if db.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "db is null".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    match (*db).inner.flush() {
        Ok(()) => KSTONE_OK,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// Put an item into the database.
///
/// # Safety
/// - All pointers must be valid
/// - `pk` and `pk_len` define the partition key
/// - `item` must be a valid KstoneItem pointer
#[no_mangle]
pub unsafe extern "C" fn kstone_db_put(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
    item: *const KstoneItem,
) -> c_int {
    clear_error();

    if db.is_null() || pk.is_null() || item.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);

    match (*db).inner.put(pk_slice, (*item).inner.clone()) {
        Ok(()) => KSTONE_OK,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

/// Put an item with a sort key into the database.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_put_with_sk(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
    sk: *const u8,
    sk_len: c_uint,
    item: *const KstoneItem,
) -> c_int {
    clear_error();

    if db.is_null() || pk.is_null() || sk.is_null() || item.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);

    match (*db).inner.put_with_sk(pk_slice, sk_slice, (*item).inner.clone()) {
        Ok(()) => KSTONE_OK,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

/// Get an item from the database.
/// Returns NULL if not found (check `kstone_last_error_code` for KSTONE_ERR_NOT_FOUND).
///
/// # Safety
/// The returned pointer must be freed with `kstone_item_free`
#[no_mangle]
pub unsafe extern "C" fn kstone_db_get(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
) -> *mut KstoneItem {
    clear_error();

    if db.is_null() || pk.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return ptr::null_mut(),
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);

    match (*db).inner.get(pk_slice) {
        Ok(Some(item)) => Box::into_raw(Box::new(KstoneItem { inner: item })),
        Ok(None) => {
            set_error(KSTONE_ERR_NOT_FOUND, "item not found".to_string());
            ptr::null_mut()
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

/// Get an item with a sort key from the database.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_get_with_sk(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
    sk: *const u8,
    sk_len: c_uint,
) -> *mut KstoneItem {
    clear_error();

    if db.is_null() || pk.is_null() || sk.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return ptr::null_mut(),
    };

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return ptr::null_mut(),
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);

    match (*db).inner.get_with_sk(pk_slice, sk_slice) {
        Ok(Some(item)) => Box::into_raw(Box::new(KstoneItem { inner: item })),
        Ok(None) => {
            set_error(KSTONE_ERR_NOT_FOUND, "item not found".to_string());
            ptr::null_mut()
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

/// Delete an item from the database.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_delete(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
) -> c_int {
    clear_error();

    if db.is_null() || pk.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);

    match (*db).inner.delete(pk_slice) {
        Ok(()) => KSTONE_OK,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

/// Delete an item with a sort key from the database.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_delete_with_sk(
    db: *mut KstoneDb,
    pk: *const u8,
    pk_len: c_uint,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    clear_error();

    if db.is_null() || pk.is_null() || sk.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);

    match (*db).inner.delete_with_sk(pk_slice, sk_slice) {
        Ok(()) => KSTONE_OK,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

// ============================================================================
// Item Operations
// ============================================================================

/// Create a new empty item.
#[no_mangle]
pub extern "C" fn kstone_item_new() -> *mut KstoneItem {
    Box::into_raw(Box::new(KstoneItem {
        inner: HashMap::new(),
    }))
}

/// Free an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_free(item: *mut KstoneItem) {
    if !item.is_null() {
        drop(Box::from_raw(item));
    }
}

/// Set a string attribute on an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_string(
    item: *mut KstoneItem,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    if item.is_null() || key.is_null() || value.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let value_str = match CStr::from_ptr(value).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    (*item).inner.insert(key_str, kstone_api::KeystoneValue::S(value_str));
    KSTONE_OK
}

/// Set a number attribute on an item (as integer).
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_number_int(
    item: *mut KstoneItem,
    key: *const c_char,
    value: i64,
) -> c_int {
    if item.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    (*item).inner.insert(key_str, kstone_api::KeystoneValue::N(value.to_string()));
    KSTONE_OK
}

/// Set a number attribute on an item (as double).
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_number_double(
    item: *mut KstoneItem,
    key: *const c_char,
    value: c_double,
) -> c_int {
    if item.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    (*item).inner.insert(key_str, kstone_api::KeystoneValue::N(value.to_string()));
    KSTONE_OK
}

/// Set a boolean attribute on an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_bool(
    item: *mut KstoneItem,
    key: *const c_char,
    value: c_int,
) -> c_int {
    if item.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    (*item).inner.insert(key_str, kstone_api::KeystoneValue::Bool(value != 0));
    KSTONE_OK
}

/// Set a null attribute on an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_null(
    item: *mut KstoneItem,
    key: *const c_char,
) -> c_int {
    if item.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    (*item).inner.insert(key_str, kstone_api::KeystoneValue::Null);
    KSTONE_OK
}

/// Set a binary attribute on an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_set_binary(
    item: *mut KstoneItem,
    key: *const c_char,
    value: *const u8,
    value_len: c_uint,
) -> c_int {
    if item.is_null() || key.is_null() || value.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s.to_string(),
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let value_len_validated = match validate_value_size(value_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let bytes = std::slice::from_raw_parts(value, value_len_validated).to_vec();
    (*item).inner.insert(key_str, kstone_api::KeystoneValue::B(bytes.into()));
    KSTONE_OK
}

/// Get a string attribute from an item.
/// Returns NULL if the attribute doesn't exist or isn't a string.
/// The returned string must be freed with `kstone_string_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_get_string(
    item: *const KstoneItem,
    key: *const c_char,
) -> *mut c_char {
    if item.is_null() || key.is_null() {
        return ptr::null_mut();
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return ptr::null_mut(),
    };

    match (*item).inner.get(key_str) {
        Some(kstone_api::KeystoneValue::S(s)) => {
            match CString::new(s.as_str()) {
                Ok(cstr) => cstr.into_raw(),
                Err(_) => ptr::null_mut(),
            }
        }
        _ => ptr::null_mut(),
    }
}

/// Get a number attribute from an item as a double.
/// Returns 0.0 if the attribute doesn't exist or isn't a number.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_get_number(
    item: *const KstoneItem,
    key: *const c_char,
    out_value: *mut c_double,
) -> c_int {
    if item.is_null() || key.is_null() || out_value.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    match (*item).inner.get(key_str) {
        Some(kstone_api::KeystoneValue::N(n)) => {
            match n.parse::<f64>() {
                Ok(v) => {
                    *out_value = v;
                    KSTONE_OK
                }
                Err(_) => KSTONE_ERR_INVALID_ARGUMENT,
            }
        }
        _ => KSTONE_ERR_NOT_FOUND,
    }
}

/// Get a boolean attribute from an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_get_bool(
    item: *const KstoneItem,
    key: *const c_char,
    out_value: *mut c_int,
) -> c_int {
    if item.is_null() || key.is_null() || out_value.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    match (*item).inner.get(key_str) {
        Some(kstone_api::KeystoneValue::Bool(b)) => {
            *out_value = if *b { 1 } else { 0 };
            KSTONE_OK
        }
        _ => KSTONE_ERR_NOT_FOUND,
    }
}

/// Check if an attribute exists in an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_has_attribute(
    item: *const KstoneItem,
    key: *const c_char,
) -> c_int {
    if item.is_null() || key.is_null() {
        return 0;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return 0,
    };

    if (*item).inner.contains_key(key_str) { 1 } else { 0 }
}

/// Get the number of attributes in an item.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_attribute_count(item: *const KstoneItem) -> c_uint {
    if item.is_null() {
        return 0;
    }
    (*item).inner.len() as c_uint
}

/// Convert an item to JSON string.
/// The returned string must be freed with `kstone_string_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_to_json(item: *const KstoneItem) -> *mut c_char {
    if item.is_null() {
        return ptr::null_mut();
    }

    // Convert KeystoneValue to serde_json::Value for serialization
    fn value_to_json(v: &kstone_api::KeystoneValue) -> serde_json::Value {
        match v {
            kstone_api::KeystoneValue::S(s) => serde_json::json!(s),
            kstone_api::KeystoneValue::N(n) => {
                if let Ok(i) = n.parse::<i64>() {
                    serde_json::json!(i)
                } else if let Ok(f) = n.parse::<f64>() {
                    serde_json::json!(f)
                } else {
                    serde_json::json!(n)
                }
            }
            kstone_api::KeystoneValue::B(b) => {
                serde_json::json!(base64::Engine::encode(&base64::engine::general_purpose::STANDARD, b.as_ref()))
            }
            kstone_api::KeystoneValue::Bool(b) => serde_json::json!(b),
            kstone_api::KeystoneValue::Null => serde_json::Value::Null,
            kstone_api::KeystoneValue::L(list) => {
                serde_json::Value::Array(list.iter().map(value_to_json).collect())
            }
            kstone_api::KeystoneValue::M(map) => {
                let obj: serde_json::Map<String, serde_json::Value> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), value_to_json(v)))
                    .collect();
                serde_json::Value::Object(obj)
            }
            kstone_api::KeystoneValue::VecF32(vec) => serde_json::json!(vec),
            kstone_api::KeystoneValue::Ts(ts) => serde_json::json!(ts),
            // Handle any future variants (KeystoneValue is non-exhaustive)
            _ => serde_json::Value::Null,
        }
    }

    let json_map: serde_json::Map<String, serde_json::Value> = (*item)
        .inner
        .iter()
        .map(|(k, v)| (k.clone(), value_to_json(v)))
        .collect();

    match serde_json::to_string(&serde_json::Value::Object(json_map)) {
        Ok(s) => match CString::new(s) {
            Ok(cstr) => cstr.into_raw(),
            Err(_) => ptr::null_mut(),
        },
        Err(_) => ptr::null_mut(),
    }
}

/// Create an item from JSON string.
/// The returned item must be freed with `kstone_item_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_from_json(json: *const c_char) -> *mut KstoneItem {
    if json.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "json is null".to_string());
        return ptr::null_mut();
    }

    let json_str = match CStr::from_ptr(json).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(KSTONE_ERR_INVALID_UTF8, format!("invalid UTF-8: {}", e));
            return ptr::null_mut();
        }
    };

    fn json_to_value(v: &serde_json::Value) -> Option<kstone_api::KeystoneValue> {
        match v {
            serde_json::Value::String(s) => Some(kstone_api::KeystoneValue::S(s.clone())),
            serde_json::Value::Number(n) => Some(kstone_api::KeystoneValue::N(n.to_string())),
            serde_json::Value::Bool(b) => Some(kstone_api::KeystoneValue::Bool(*b)),
            serde_json::Value::Null => Some(kstone_api::KeystoneValue::Null),
            serde_json::Value::Array(arr) => {
                let list: Option<Vec<_>> = arr.iter().map(json_to_value).collect();
                list.map(kstone_api::KeystoneValue::L)
            }
            serde_json::Value::Object(obj) => {
                let map: Option<HashMap<_, _>> = obj
                    .iter()
                    .map(|(k, v)| json_to_value(v).map(|val| (k.clone(), val)))
                    .collect();
                map.map(kstone_api::KeystoneValue::M)
            }
        }
    }

    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(serde_json::Value::Object(obj)) => {
            let mut item = HashMap::new();
            for (k, v) in obj {
                if let Some(val) = json_to_value(&v) {
                    item.insert(k, val);
                }
            }
            Box::into_raw(Box::new(KstoneItem { inner: item }))
        }
        Ok(_) => {
            set_error(KSTONE_ERR_INVALID_ARGUMENT, "JSON must be an object".to_string());
            ptr::null_mut()
        }
        Err(e) => {
            set_error(KSTONE_ERR_INVALID_ARGUMENT, format!("invalid JSON: {}", e));
            ptr::null_mut()
        }
    }
}

// ============================================================================
// ItemBuilder Operations
// ============================================================================

/// Create a new ItemBuilder.
#[no_mangle]
pub extern "C" fn kstone_item_builder_new() -> *mut KstoneItemBuilder {
    Box::into_raw(Box::new(KstoneItemBuilder {
        inner: ItemBuilder::new(),
    }))
}

/// Free an ItemBuilder.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_builder_free(builder: *mut KstoneItemBuilder) {
    if !builder.is_null() {
        drop(Box::from_raw(builder));
    }
}

/// Add a string attribute to the builder.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_builder_string(
    builder: *mut KstoneItemBuilder,
    key: *const c_char,
    value: *const c_char,
) -> c_int {
    if builder.is_null() || key.is_null() || value.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let value_str = match CStr::from_ptr(value).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    // Take ownership, add attribute, put back
    let old_builder = std::mem::replace(&mut (*builder).inner, ItemBuilder::new());
    (*builder).inner = old_builder.string(key_str, value_str);
    KSTONE_OK
}

/// Add a number attribute to the builder.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_builder_number(
    builder: *mut KstoneItemBuilder,
    key: *const c_char,
    value: i64,
) -> c_int {
    if builder.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let old_builder = std::mem::replace(&mut (*builder).inner, ItemBuilder::new());
    (*builder).inner = old_builder.number(key_str, value);
    KSTONE_OK
}

/// Add a boolean attribute to the builder.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_builder_bool(
    builder: *mut KstoneItemBuilder,
    key: *const c_char,
    value: c_int,
) -> c_int {
    if builder.is_null() || key.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let key_str = match CStr::from_ptr(key).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let old_builder = std::mem::replace(&mut (*builder).inner, ItemBuilder::new());
    (*builder).inner = old_builder.bool(key_str, value != 0);
    KSTONE_OK
}

/// Build the item from the builder. Consumes the builder.
/// Returns NULL on error. The returned item must be freed with `kstone_item_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_item_builder_build(
    builder: *mut KstoneItemBuilder,
) -> *mut KstoneItem {
    if builder.is_null() {
        return ptr::null_mut();
    }

    let builder_box = Box::from_raw(builder);
    let item = builder_box.inner.build();
    Box::into_raw(Box::new(KstoneItem { inner: item }))
}

// ============================================================================
// Query Operations
// ============================================================================

/// Create a new Query for the given partition key.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_new(
    pk: *const u8,
    pk_len: c_uint,
) -> *mut KstoneQuery {
    if pk.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "pk is null".to_string());
        return ptr::null_mut();
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return ptr::null_mut(),
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    Box::into_raw(Box::new(KstoneQuery {
        inner: Query::new(pk_slice),
    }))
}

/// Free a Query.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_free(query: *mut KstoneQuery) {
    if !query.is_null() {
        drop(Box::from_raw(query));
    }
}

/// Set sort key equals condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_eq(
    query: *mut KstoneQuery,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    if query.is_null() || sk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_eq(sk_slice);
    KSTONE_OK
}

/// Set sort key begins_with condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_begins_with(
    query: *mut KstoneQuery,
    prefix: *const u8,
    prefix_len: c_uint,
) -> c_int {
    if query.is_null() || prefix.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let prefix_len_validated = match validate_key_size(prefix_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let prefix_slice = std::slice::from_raw_parts(prefix, prefix_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_begins_with(prefix_slice);
    KSTONE_OK
}

/// Set sort key greater than condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_gt(
    query: *mut KstoneQuery,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    if query.is_null() || sk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_gt(sk_slice);
    KSTONE_OK
}

/// Set sort key greater than or equal condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_gte(
    query: *mut KstoneQuery,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    if query.is_null() || sk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_gte(sk_slice);
    KSTONE_OK
}

/// Set sort key less than condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_lt(
    query: *mut KstoneQuery,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    if query.is_null() || sk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_lt(sk_slice);
    KSTONE_OK
}

/// Set sort key less than or equal condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_lte(
    query: *mut KstoneQuery,
    sk: *const u8,
    sk_len: c_uint,
) -> c_int {
    if query.is_null() || sk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk_len_validated = match validate_key_size(sk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk_slice = std::slice::from_raw_parts(sk, sk_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_lte(sk_slice);
    KSTONE_OK
}

/// Set sort key between condition.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_sk_between(
    query: *mut KstoneQuery,
    sk1: *const u8,
    sk1_len: c_uint,
    sk2: *const u8,
    sk2_len: c_uint,
) -> c_int {
    if query.is_null() || sk1.is_null() || sk2.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let sk1_len_validated = match validate_key_size(sk1_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk2_len_validated = match validate_key_size(sk2_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let sk1_slice = std::slice::from_raw_parts(sk1, sk1_len_validated);
    let sk2_slice = std::slice::from_raw_parts(sk2, sk2_len_validated);
    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.sk_between(sk1_slice, sk2_slice);
    KSTONE_OK
}

/// Set query limit.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_limit(query: *mut KstoneQuery, limit: c_uint) -> c_int {
    if query.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.limit(limit as usize);
    KSTONE_OK
}

/// Set query direction (1 = forward, 0 = reverse).
#[no_mangle]
pub unsafe extern "C" fn kstone_query_forward(query: *mut KstoneQuery, forward: c_int) -> c_int {
    if query.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.forward(forward != 0);
    KSTONE_OK
}

/// Set index name for the query.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_index(
    query: *mut KstoneQuery,
    index_name: *const c_char,
) -> c_int {
    if query.is_null() || index_name.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let index_str = match CStr::from_ptr(index_name).to_str() {
        Ok(s) => s,
        Err(_) => return KSTONE_ERR_INVALID_UTF8,
    };

    let old_query = std::mem::replace(&mut (*query).inner, Query::new(&[]));
    (*query).inner = old_query.index(index_str);
    KSTONE_OK
}

/// Execute a query. The query is consumed.
/// Returns NULL on error. The response must be freed with `kstone_query_response_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_query(
    db: *mut KstoneDb,
    query: *mut KstoneQuery,
) -> *mut KstoneQueryResponse {
    clear_error();

    if db.is_null() || query.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let query_box = Box::from_raw(query);

    match (*db).inner.query(query_box.inner) {
        Ok(response) => {
            let item_count = response.items.len();
            let items: Vec<*mut KstoneItem> = response
                .items
                .into_iter()
                .map(|item| Box::into_raw(Box::new(KstoneItem { inner: item })))
                .collect();

            let items_ptr = if items.is_empty() {
                ptr::null_mut()
            } else {
                let boxed = items.into_boxed_slice();
                Box::into_raw(boxed) as *mut *mut KstoneItem
            };

            let has_more = response.last_key.is_some();
            let (last_pk, last_sk) = match response.last_key {
                Some((pk, sk)) => {
                    let pk_str = CString::new(pk.to_vec())
                        .map(|c| c.into_raw())
                        .unwrap_or(ptr::null_mut());
                    let sk_str = sk.and_then(|s| CString::new(s.to_vec()).ok())
                        .map(|c| c.into_raw())
                        .unwrap_or(ptr::null_mut());
                    (pk_str, sk_str)
                }
                None => (ptr::null_mut(), ptr::null_mut()),
            };

            Box::into_raw(Box::new(KstoneQueryResponse {
                items: items_ptr,
                item_count: item_count as c_uint,
                scanned_count: response.scanned_count as c_uint,
                has_more: if has_more { 1 } else { 0 },
                last_pk,
                last_sk,
            }))
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

/// Free a query response.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_response_free(response: *mut KstoneQueryResponse) {
    if response.is_null() {
        return;
    }

    let response = Box::from_raw(response);

    // Free items
    if !response.items.is_null() && response.item_count > 0 {
        let items = std::slice::from_raw_parts_mut(response.items, response.item_count as usize);
        for item in items {
            if !item.is_null() {
                drop(Box::from_raw(*item));
            }
        }
        drop(Box::from_raw(response.items as *mut [*mut KstoneItem; 0]));
    }

    // Free strings
    if !response.last_pk.is_null() {
        drop(CString::from_raw(response.last_pk));
    }
    if !response.last_sk.is_null() {
        drop(CString::from_raw(response.last_sk));
    }
}

/// Get an item from a query response by index.
#[no_mangle]
pub unsafe extern "C" fn kstone_query_response_get_item(
    response: *const KstoneQueryResponse,
    index: c_uint,
) -> *const KstoneItem {
    if response.is_null() {
        return ptr::null();
    }

    if index >= (*response).item_count {
        return ptr::null();
    }

    let items = std::slice::from_raw_parts((*response).items, (*response).item_count as usize);
    items[index as usize]
}

// ============================================================================
// Scan Operations
// ============================================================================

/// Create a new Scan.
#[no_mangle]
pub extern "C" fn kstone_scan_new() -> *mut KstoneScan {
    Box::into_raw(Box::new(KstoneScan {
        inner: Scan::new(),
    }))
}

/// Free a Scan.
#[no_mangle]
pub unsafe extern "C" fn kstone_scan_free(scan: *mut KstoneScan) {
    if !scan.is_null() {
        drop(Box::from_raw(scan));
    }
}

/// Set scan limit.
#[no_mangle]
pub unsafe extern "C" fn kstone_scan_limit(scan: *mut KstoneScan, limit: c_uint) -> c_int {
    if scan.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let old_scan = std::mem::replace(&mut (*scan).inner, Scan::new());
    (*scan).inner = old_scan.limit(limit as usize);
    KSTONE_OK
}

/// Set scan segment for parallel scans.
#[no_mangle]
pub unsafe extern "C" fn kstone_scan_segment(
    scan: *mut KstoneScan,
    segment: c_uint,
    total_segments: c_uint,
) -> c_int {
    if scan.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let old_scan = std::mem::replace(&mut (*scan).inner, Scan::new());
    (*scan).inner = old_scan.segment(segment as usize, total_segments as usize);
    KSTONE_OK
}

/// Execute a scan. The scan is consumed.
/// Returns NULL on error. The response must be freed with `kstone_query_response_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_scan(
    db: *mut KstoneDb,
    scan: *mut KstoneScan,
) -> *mut KstoneQueryResponse {
    clear_error();

    if db.is_null() || scan.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let scan_box = Box::from_raw(scan);

    match (*db).inner.scan(scan_box.inner) {
        Ok(response) => {
            let item_count = response.items.len();
            let items: Vec<*mut KstoneItem> = response
                .items
                .into_iter()
                .map(|item| Box::into_raw(Box::new(KstoneItem { inner: item })))
                .collect();

            let items_ptr = if items.is_empty() {
                ptr::null_mut()
            } else {
                let boxed = items.into_boxed_slice();
                Box::into_raw(boxed) as *mut *mut KstoneItem
            };

            let has_more = response.last_key.is_some();
            let (last_pk, last_sk) = match response.last_key {
                Some((pk, sk)) => {
                    let pk_str = CString::new(pk.to_vec())
                        .map(|c| c.into_raw())
                        .unwrap_or(ptr::null_mut());
                    let sk_str = sk.and_then(|s| CString::new(s.to_vec()).ok())
                        .map(|c| c.into_raw())
                        .unwrap_or(ptr::null_mut());
                    (pk_str, sk_str)
                }
                None => (ptr::null_mut(), ptr::null_mut()),
            };

            Box::into_raw(Box::new(KstoneQueryResponse {
                items: items_ptr,
                item_count: item_count as c_uint,
                scanned_count: response.scanned_count as c_uint,
                has_more: if has_more { 1 } else { 0 },
                last_pk,
                last_sk,
            }))
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

// ============================================================================
// PartiQL / SQL Operations
// ============================================================================

/// Execute a PartiQL/SQL statement.
/// Returns a JSON string with the result. Must be freed with `kstone_string_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_execute_sql(
    db: *mut KstoneDb,
    sql: *const c_char,
) -> *mut c_char {
    clear_error();

    if db.is_null() || sql.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let sql_str = match CStr::from_ptr(sql).to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(KSTONE_ERR_INVALID_UTF8, format!("invalid UTF-8: {}", e));
            return ptr::null_mut();
        }
    };

    match (*db).inner.execute_statement(sql_str) {
        Ok(response) => {
            // Convert response to JSON
            let json = match response {
                kstone_api::ExecuteStatementResponse::Select { items, count, .. } => {
                    serde_json::json!({
                        "type": "select",
                        "count": count,
                        "items": items.iter().map(|item| {
                            item.iter().map(|(k, v)| {
                                (k.clone(), format!("{:?}", v))
                            }).collect::<HashMap<_, _>>()
                        }).collect::<Vec<_>>()
                    })
                }
                kstone_api::ExecuteStatementResponse::Insert { success } => {
                    serde_json::json!({
                        "type": "insert",
                        "success": success
                    })
                }
                kstone_api::ExecuteStatementResponse::Update { .. } => {
                    serde_json::json!({
                        "type": "update",
                        "success": true
                    })
                }
                kstone_api::ExecuteStatementResponse::Delete { success } => {
                    serde_json::json!({
                        "type": "delete",
                        "success": success
                    })
                }
                _ => {
                    // Handle any future response types
                    serde_json::json!({
                        "type": "unknown",
                        "success": false
                    })
                }
            };

            match serde_json::to_string(&json) {
                Ok(s) => match CString::new(s) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        set_error(KSTONE_ERR_INTERNAL, "JSON contains null byte".to_string());
                        ptr::null_mut()
                    }
                },
                Err(e) => {
                    set_error(KSTONE_ERR_INTERNAL, format!("JSON serialization failed: {}", e));
                    ptr::null_mut()
                }
            }
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

// ============================================================================
// Batch Operations
// ============================================================================

/// Create a new BatchGetRequest.
#[no_mangle]
pub extern "C" fn kstone_batch_get_new() -> *mut KstoneBatchGet {
    Box::into_raw(Box::new(KstoneBatchGet {
        inner: BatchGetRequest::new(),
    }))
}

/// Free a BatchGetRequest.
#[no_mangle]
pub unsafe extern "C" fn kstone_batch_get_free(batch: *mut KstoneBatchGet) {
    if !batch.is_null() {
        drop(Box::from_raw(batch));
    }
}

/// Add a key to the batch get request.
#[no_mangle]
pub unsafe extern "C" fn kstone_batch_get_add_key(
    batch: *mut KstoneBatchGet,
    pk: *const u8,
    pk_len: c_uint,
) -> c_int {
    if batch.is_null() || pk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let old_batch = std::mem::replace(&mut (*batch).inner, BatchGetRequest::new());
    (*batch).inner = old_batch.add_key(pk_slice);
    KSTONE_OK
}

/// Execute a batch get. The request is consumed.
/// Returns a JSON string with results. Must be freed with `kstone_string_free`.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_batch_get(
    db: *mut KstoneDb,
    batch: *mut KstoneBatchGet,
) -> *mut c_char {
    clear_error();

    if db.is_null() || batch.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return ptr::null_mut();
    }

    let batch_box = Box::from_raw(batch);

    match (*db).inner.batch_get(batch_box.inner) {
        Ok(response) => {
            let json = serde_json::json!({
                "items": response.items.iter().map(|(k, v)| {
                    let key_str = String::from_utf8_lossy(&k.pk).to_string();
                    (key_str, v.iter().map(|(k, v)| {
                        (k.clone(), format!("{:?}", v))
                    }).collect::<HashMap<_, _>>())
                }).collect::<HashMap<_, _>>()
            });

            match serde_json::to_string(&json) {
                Ok(s) => match CString::new(s) {
                    Ok(cstr) => cstr.into_raw(),
                    Err(_) => {
                        set_error(KSTONE_ERR_INTERNAL, "JSON contains null byte".to_string());
                        ptr::null_mut()
                    }
                },
                Err(e) => {
                    set_error(KSTONE_ERR_INTERNAL, format!("JSON serialization failed: {}", e));
                    ptr::null_mut()
                }
            }
        }
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            ptr::null_mut()
        }
    }
}

/// Create a new BatchWriteRequest.
#[no_mangle]
pub extern "C" fn kstone_batch_write_new() -> *mut KstoneBatchWrite {
    Box::into_raw(Box::new(KstoneBatchWrite {
        inner: BatchWriteRequest::new(),
    }))
}

/// Free a BatchWriteRequest.
#[no_mangle]
pub unsafe extern "C" fn kstone_batch_write_free(batch: *mut KstoneBatchWrite) {
    if !batch.is_null() {
        drop(Box::from_raw(batch));
    }
}

/// Add a put operation to the batch write request.
#[no_mangle]
pub unsafe extern "C" fn kstone_batch_write_put(
    batch: *mut KstoneBatchWrite,
    pk: *const u8,
    pk_len: c_uint,
    item: *const KstoneItem,
) -> c_int {
    if batch.is_null() || pk.is_null() || item.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let old_batch = std::mem::replace(&mut (*batch).inner, BatchWriteRequest::new());
    (*batch).inner = old_batch.put(pk_slice, (*item).inner.clone());
    KSTONE_OK
}

/// Add a delete operation to the batch write request.
#[no_mangle]
pub unsafe extern "C" fn kstone_batch_write_delete(
    batch: *mut KstoneBatchWrite,
    pk: *const u8,
    pk_len: c_uint,
) -> c_int {
    if batch.is_null() || pk.is_null() {
        return KSTONE_ERR_NULL_POINTER;
    }

    let pk_len_validated = match validate_key_size(pk_len) {
        Some(len) => len,
        None => return KSTONE_ERR_INVALID_ARGUMENT,
    };

    let pk_slice = std::slice::from_raw_parts(pk, pk_len_validated);
    let old_batch = std::mem::replace(&mut (*batch).inner, BatchWriteRequest::new());
    (*batch).inner = old_batch.delete(pk_slice);
    KSTONE_OK
}

/// Execute a batch write. The request is consumed.
/// Returns the number of processed items, or a negative error code.
#[no_mangle]
pub unsafe extern "C" fn kstone_db_batch_write(
    db: *mut KstoneDb,
    batch: *mut KstoneBatchWrite,
) -> c_int {
    clear_error();

    if db.is_null() || batch.is_null() {
        set_error(KSTONE_ERR_NULL_POINTER, "null pointer argument".to_string());
        return KSTONE_ERR_NULL_POINTER;
    }

    let batch_box = Box::from_raw(batch);

    match (*db).inner.batch_write(batch_box.inner) {
        Ok(response) => response.processed_count as c_int,
        Err(e) => {
            let code = error_from_keystone(&e);
            set_error(code, e.to_string());
            code
        }
    }
}

// ============================================================================
// Version and Info
// ============================================================================

/// Get the version of the KeystoneDB library.
///
/// # Safety
/// The returned string is statically allocated and must not be freed.
#[no_mangle]
pub extern "C" fn kstone_version() -> *const c_char {
    static VERSION: &[u8] = b"0.1.0\0";
    VERSION.as_ptr() as *const c_char
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    #[test]
    fn test_version() {
        let version = kstone_version();
        assert!(!version.is_null());
        unsafe {
            let version_str = CStr::from_ptr(version).to_str().unwrap_or("<invalid utf-8>");
            assert_eq!(version_str, "0.1.0");
        }
    }

    #[test]
    fn test_error_handling() {
        clear_error();
        assert_eq!(kstone_last_error_code(), KSTONE_OK);

        set_error(KSTONE_ERR_NOT_FOUND, "test error".to_string());
        assert_eq!(kstone_last_error_code(), KSTONE_ERR_NOT_FOUND);

        let msg = kstone_last_error();
        assert!(!msg.is_null());
        unsafe {
            let msg_str = CStr::from_ptr(msg).to_str().unwrap_or("<invalid utf-8>");
            assert_eq!(msg_str, "test error");
            kstone_string_free(msg);
        }

        clear_error();
        assert_eq!(kstone_last_error_code(), KSTONE_OK);
    }

    #[test]
    fn test_item_builder() {
        let builder = kstone_item_builder_new();
        assert!(!builder.is_null());

        unsafe {
            let key = CString::new("name").unwrap();
            let value = CString::new("Alice").unwrap();
            assert_eq!(
                kstone_item_builder_string(builder, key.as_ptr(), value.as_ptr()),
                KSTONE_OK
            );

            let key = CString::new("age").unwrap();
            assert_eq!(kstone_item_builder_number(builder, key.as_ptr(), 30), KSTONE_OK);

            let item = kstone_item_builder_build(builder);
            assert!(!item.is_null());

            // Verify the item
            let name_key = CString::new("name").unwrap();
            let name_value = kstone_item_get_string(item, name_key.as_ptr());
            assert!(!name_value.is_null());
            assert_eq!(CStr::from_ptr(name_value).to_str().unwrap_or("<invalid utf-8>"), "Alice");
            kstone_string_free(name_value);

            let age_key = CString::new("age").unwrap();
            let mut age_value: f64 = 0.0;
            assert_eq!(
                kstone_item_get_number(item, age_key.as_ptr(), &mut age_value),
                KSTONE_OK
            );
            assert_eq!(age_value, 30.0);

            kstone_item_free(item);
        }
    }

    #[test]
    fn test_item_json_roundtrip() {
        unsafe {
            let json = CString::new(r#"{"name": "Bob", "age": 25, "active": true}"#).unwrap();
            let item = kstone_item_from_json(json.as_ptr());
            assert!(!item.is_null());

            let json_out = kstone_item_to_json(item);
            assert!(!json_out.is_null());

            // Parse and verify
            let json_str = CStr::from_ptr(json_out).to_str().unwrap_or("<invalid utf-8>");
            let parsed: serde_json::Value = serde_json::from_str(json_str).unwrap();
            assert_eq!(parsed["name"], "Bob");
            assert_eq!(parsed["age"], 25);
            assert_eq!(parsed["active"], true);

            kstone_string_free(json_out);
            kstone_item_free(item);
        }
    }

    #[test]
    fn test_in_memory_database() {
        let db = kstone_db_create_in_memory();
        assert!(!db.is_null());

        unsafe {
            // Create an item
            let item = kstone_item_new();
            let key = CString::new("name").unwrap();
            let value = CString::new("Test").unwrap();
            kstone_item_set_string(item, key.as_ptr(), value.as_ptr());

            // Put the item
            let pk = b"test#1";
            assert_eq!(
                kstone_db_put(db, pk.as_ptr(), pk.len() as c_uint, item),
                KSTONE_OK
            );

            // Get the item
            let retrieved = kstone_db_get(db, pk.as_ptr(), pk.len() as c_uint);
            assert!(!retrieved.is_null());

            // Verify
            let name_value = kstone_item_get_string(retrieved, key.as_ptr());
            assert!(!name_value.is_null());
            assert_eq!(CStr::from_ptr(name_value).to_str().unwrap_or("<invalid utf-8>"), "Test");

            kstone_string_free(name_value);
            kstone_item_free(retrieved);
            kstone_item_free(item);
            kstone_db_close(db);
        }
    }

    #[test]
    fn test_path_traversal_prevention() {
        unsafe {
            // Test path with ".." should be rejected
            let path = CString::new("../../../etc/passwd").unwrap();
            let db = kstone_db_create(path.as_ptr());
            assert!(db.is_null());
            assert_eq!(kstone_last_error_code(), KSTONE_ERR_INVALID_ARGUMENT);

            let err_msg = kstone_last_error();
            assert!(!err_msg.is_null());
            let msg_str = CStr::from_ptr(err_msg).to_str().unwrap_or("<invalid utf-8>");
            assert!(msg_str.contains("Path traversal not allowed"));
            kstone_string_free(err_msg);

            // Test open with path traversal
            let path = CString::new("safe/../dangerous").unwrap();
            let db = kstone_db_open(path.as_ptr());
            assert!(db.is_null());
            assert_eq!(kstone_last_error_code(), KSTONE_ERR_INVALID_ARGUMENT);

            // Test valid path should work (even if the database doesn't exist)
            let path = CString::new("valid/path/to/db").unwrap();
            let db = kstone_db_create(path.as_ptr());
            // This might fail for other reasons (like directory doesn't exist),
            // but it should NOT fail with path traversal error
            let last_code = kstone_last_error_code();
            assert_ne!(last_code, KSTONE_ERR_NULL_POINTER);
            if !db.is_null() {
                kstone_db_close(db);
            }
        }
    }
}
