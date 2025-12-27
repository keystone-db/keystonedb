//! Python bindings for KeystoneDB
//!
//! This module provides Python bindings using PyO3, exposing KeystoneDB
//! as a native Python module with Pythonic API.

use pyo3::prelude::*;
use pyo3::exceptions::{PyIOError, PyKeyError, PyRuntimeError, PyValueError};
use pyo3::types::{PyBytes, PyDict, PyList};
use std::collections::HashMap;

use kstone_api::{
    BatchGetRequest, BatchWriteRequest, Database as KstoneDatabase,
    Query as KstoneQuery, Scan as KstoneScan, Update as KstoneUpdate,
    KeystoneError, KeystoneValue,
};

/// Convert a KeystoneError to a Python exception
fn to_py_error(err: KeystoneError) -> PyErr {
    match err {
        KeystoneError::NotFound(msg) => PyKeyError::new_err(msg),
        KeystoneError::InvalidQuery(msg) => PyValueError::new_err(msg),
        KeystoneError::InvalidArgument(msg) => PyValueError::new_err(msg),
        KeystoneError::ConditionalCheckFailed(msg) => PyRuntimeError::new_err(format!("Condition check failed: {}", msg)),
        KeystoneError::TransactionCanceled(msg) => PyRuntimeError::new_err(format!("Transaction canceled: {}", msg)),
        KeystoneError::Io(err) => PyIOError::new_err(err.to_string()),
        KeystoneError::Corruption(msg) => PyRuntimeError::new_err(format!("Data corruption: {}", msg)),
        _ => PyRuntimeError::new_err(err.to_string()),
    }
}

/// Convert a Python object to a KeystoneValue
fn py_to_value(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<KeystoneValue> {
    if obj.is_none() {
        return Ok(KeystoneValue::Null);
    }

    if let Ok(s) = obj.extract::<String>() {
        return Ok(KeystoneValue::S(s));
    }

    if let Ok(b) = obj.extract::<bool>() {
        return Ok(KeystoneValue::Bool(b));
    }

    if let Ok(i) = obj.extract::<i64>() {
        return Ok(KeystoneValue::N(i.to_string()));
    }

    if let Ok(f) = obj.extract::<f64>() {
        return Ok(KeystoneValue::N(f.to_string()));
    }

    if let Ok(bytes) = obj.extract::<Vec<u8>>() {
        return Ok(KeystoneValue::B(bytes.into()));
    }

    if let Ok(list) = obj.downcast::<PyList>() {
        let values: PyResult<Vec<KeystoneValue>> = list.iter()
            .map(|item| py_to_value(py, &item))
            .collect();
        return Ok(KeystoneValue::L(values?));
    }

    if let Ok(dict) = obj.downcast::<PyDict>() {
        let map: PyResult<HashMap<String, KeystoneValue>> = dict.iter()
            .map(|(k, v)| {
                let key: String = k.extract()?;
                let value = py_to_value(py, &v)?;
                Ok((key, value))
            })
            .collect();
        return Ok(KeystoneValue::M(map?));
    }

    // Try extracting as a list of f32 for vectors
    if let Ok(vec) = obj.extract::<Vec<f32>>() {
        return Ok(KeystoneValue::VecF32(vec));
    }

    Err(PyValueError::new_err("Unsupported value type"))
}

/// Convert a KeystoneValue to a Python object
fn value_to_py(py: Python<'_>, value: &KeystoneValue) -> PyObject {
    match value {
        KeystoneValue::S(s) => s.to_object(py),
        KeystoneValue::N(n) => {
            if let Ok(i) = n.parse::<i64>() {
                i.to_object(py)
            } else if let Ok(f) = n.parse::<f64>() {
                f.to_object(py)
            } else {
                n.to_object(py)
            }
        }
        KeystoneValue::B(b) => PyBytes::new_bound(py, b.as_ref()).to_object(py),
        KeystoneValue::Bool(b) => b.to_object(py),
        KeystoneValue::Null => py.None(),
        KeystoneValue::L(list) => {
            let py_items: Vec<PyObject> = list.iter().map(|v| value_to_py(py, v)).collect();
            PyList::new_bound(py, py_items).to_object(py)
        }
        KeystoneValue::M(map) => {
            let py_dict = PyDict::new_bound(py);
            for (k, v) in map {
                py_dict.set_item(k, value_to_py(py, v)).unwrap();
            }
            py_dict.to_object(py)
        }
        KeystoneValue::VecF32(vec) => vec.to_object(py),
        KeystoneValue::Ts(ts) => ts.to_object(py),
    }
}

/// Convert a Python dict to a KeystoneDB item
fn dict_to_item(py: Python<'_>, dict: &Bound<'_, PyDict>) -> PyResult<HashMap<String, KeystoneValue>> {
    dict.iter()
        .map(|(k, v)| {
            let key: String = k.extract()?;
            let value = py_to_value(py, &v)?;
            Ok((key, value))
        })
        .collect()
}

/// Convert a KeystoneDB item to a Python dict
fn item_to_dict(py: Python<'_>, item: &HashMap<String, KeystoneValue>) -> PyObject {
    let dict = PyDict::new_bound(py);
    for (k, v) in item {
        dict.set_item(k, value_to_py(py, v)).unwrap();
    }
    dict.to_object(py)
}

/// KeystoneDB Database
///
/// An embedded DynamoDB-compatible database for Python.
///
/// Example:
///     >>> from keystonedb import Database
///     >>> db = Database.create("mydb.keystone")
///     >>> db.put(b"user#123", {"name": "Alice", "age": 30})
///     >>> item = db.get(b"user#123")
///     >>> print(item["name"])
///     Alice
#[pyclass]
struct Database {
    inner: KstoneDatabase,
}

#[pymethods]
impl Database {
    /// Create a new database at the given path.
    ///
    /// Args:
    ///     path: Path to the database directory
    ///
    /// Returns:
    ///     A new Database instance
    ///
    /// Raises:
    ///     IOError: If the database cannot be created
    #[staticmethod]
    fn create(path: &str) -> PyResult<Self> {
        let db = KstoneDatabase::create(path).map_err(to_py_error)?;
        Ok(Database { inner: db })
    }

    /// Open an existing database at the given path.
    ///
    /// Args:
    ///     path: Path to the database directory
    ///
    /// Returns:
    ///     A Database instance
    ///
    /// Raises:
    ///     IOError: If the database cannot be opened
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let db = KstoneDatabase::open(path).map_err(to_py_error)?;
        Ok(Database { inner: db })
    }

    /// Create an in-memory database (no persistence).
    ///
    /// Returns:
    ///     A new in-memory Database instance
    #[staticmethod]
    fn create_in_memory() -> PyResult<Self> {
        let db = KstoneDatabase::create_in_memory().map_err(to_py_error)?;
        Ok(Database { inner: db })
    }

    /// Put an item into the database.
    ///
    /// Args:
    ///     pk: Partition key (bytes or string)
    ///     item: Item data as a dictionary
    ///     sk: Optional sort key (bytes or string)
    ///
    /// Raises:
    ///     IOError: If the write fails
    #[pyo3(signature = (pk, item, sk=None))]
    fn put(&self, py: Python<'_>, pk: &Bound<'_, PyAny>, item: &Bound<'_, PyDict>, sk: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let pk_bytes = extract_key(pk)?;
        let item_map = dict_to_item(py, item)?;

        match sk {
            Some(sk_val) => {
                let sk_bytes = extract_key(sk_val)?;
                self.inner.put_with_sk(&pk_bytes, &sk_bytes, item_map).map_err(to_py_error)
            }
            None => self.inner.put(&pk_bytes, item_map).map_err(to_py_error)
        }
    }

    /// Get an item from the database.
    ///
    /// Args:
    ///     pk: Partition key (bytes or string)
    ///     sk: Optional sort key (bytes or string)
    ///
    /// Returns:
    ///     Item data as a dictionary, or None if not found
    #[pyo3(signature = (pk, sk=None))]
    fn get(&self, py: Python<'_>, pk: &Bound<'_, PyAny>, sk: Option<&Bound<'_, PyAny>>) -> PyResult<Option<PyObject>> {
        let pk_bytes = extract_key(pk)?;

        let result = match sk {
            Some(sk_val) => {
                let sk_bytes = extract_key(sk_val)?;
                self.inner.get_with_sk(&pk_bytes, &sk_bytes)
            }
            None => self.inner.get(&pk_bytes)
        };

        match result {
            Ok(Some(item)) => Ok(Some(item_to_dict(py, &item))),
            Ok(None) => Ok(None),
            Err(e) => Err(to_py_error(e))
        }
    }

    /// Delete an item from the database.
    ///
    /// Args:
    ///     pk: Partition key (bytes or string)
    ///     sk: Optional sort key (bytes or string)
    ///
    /// Raises:
    ///     IOError: If the delete fails
    #[pyo3(signature = (pk, sk=None))]
    fn delete(&self, pk: &Bound<'_, PyAny>, sk: Option<&Bound<'_, PyAny>>) -> PyResult<()> {
        let pk_bytes = extract_key(pk)?;

        match sk {
            Some(sk_val) => {
                let sk_bytes = extract_key(sk_val)?;
                self.inner.delete_with_sk(&pk_bytes, &sk_bytes).map_err(to_py_error)
            }
            None => self.inner.delete(&pk_bytes).map_err(to_py_error)
        }
    }

    /// Query items by partition key with optional sort key conditions.
    ///
    /// Args:
    ///     pk: Partition key (bytes or string)
    ///     sk_begins_with: Filter items where sort key starts with this prefix
    ///     sk_eq: Filter items where sort key equals this value
    ///     sk_gt: Filter items where sort key is greater than this value
    ///     sk_gte: Filter items where sort key is greater than or equal to this value
    ///     sk_lt: Filter items where sort key is less than this value
    ///     sk_lte: Filter items where sort key is less than or equal to this value
    ///     limit: Maximum number of items to return
    ///     forward: True for ascending order, False for descending
    ///     index: Name of secondary index to query
    ///
    /// Returns:
    ///     List of items matching the query
    #[pyo3(signature = (pk, sk_begins_with=None, sk_eq=None, sk_gt=None, sk_gte=None, sk_lt=None, sk_lte=None, limit=None, forward=true, index=None))]
    #[allow(clippy::too_many_arguments)]
    fn query(
        &self,
        py: Python<'_>,
        pk: &Bound<'_, PyAny>,
        sk_begins_with: Option<&Bound<'_, PyAny>>,
        sk_eq: Option<&Bound<'_, PyAny>>,
        sk_gt: Option<&Bound<'_, PyAny>>,
        sk_gte: Option<&Bound<'_, PyAny>>,
        sk_lt: Option<&Bound<'_, PyAny>>,
        sk_lte: Option<&Bound<'_, PyAny>>,
        limit: Option<usize>,
        forward: bool,
        index: Option<&str>,
    ) -> PyResult<Vec<PyObject>> {
        let pk_bytes = extract_key(pk)?;
        let mut query = KstoneQuery::new(&pk_bytes);

        if let Some(prefix) = sk_begins_with {
            let prefix_bytes = extract_key(prefix)?;
            query = query.sk_begins_with(&prefix_bytes);
        }
        if let Some(eq) = sk_eq {
            let eq_bytes = extract_key(eq)?;
            query = query.sk_eq(&eq_bytes);
        }
        if let Some(gt) = sk_gt {
            let gt_bytes = extract_key(gt)?;
            query = query.sk_gt(&gt_bytes);
        }
        if let Some(gte) = sk_gte {
            let gte_bytes = extract_key(gte)?;
            query = query.sk_gte(&gte_bytes);
        }
        if let Some(lt) = sk_lt {
            let lt_bytes = extract_key(lt)?;
            query = query.sk_lt(&lt_bytes);
        }
        if let Some(lte) = sk_lte {
            let lte_bytes = extract_key(lte)?;
            query = query.sk_lte(&lte_bytes);
        }
        if let Some(lim) = limit {
            query = query.limit(lim);
        }
        query = query.forward(forward);
        if let Some(idx) = index {
            query = query.index(idx);
        }

        let response = self.inner.query(query).map_err(to_py_error)?;
        let items: Vec<PyObject> = response.items
            .iter()
            .map(|item| item_to_dict(py, item))
            .collect();
        Ok(items)
    }

    /// Scan all items in the database.
    ///
    /// Args:
    ///     limit: Maximum number of items to return
    ///     segment: Segment number for parallel scans (0-based)
    ///     total_segments: Total number of segments for parallel scans
    ///
    /// Returns:
    ///     List of all items (or items in the specified segment)
    #[pyo3(signature = (limit=None, segment=None, total_segments=None))]
    fn scan(
        &self,
        py: Python<'_>,
        limit: Option<usize>,
        segment: Option<usize>,
        total_segments: Option<usize>,
    ) -> PyResult<Vec<PyObject>> {
        let mut scan = KstoneScan::new();

        if let Some(lim) = limit {
            scan = scan.limit(lim);
        }
        if let (Some(seg), Some(total)) = (segment, total_segments) {
            scan = scan.segment(seg, total);
        }

        let response = self.inner.scan(scan).map_err(to_py_error)?;
        let items: Vec<PyObject> = response.items
            .iter()
            .map(|item| item_to_dict(py, item))
            .collect();
        Ok(items)
    }

    /// Update an item in the database using an update expression.
    ///
    /// Args:
    ///     pk: Partition key (bytes or string)
    ///     expression: Update expression (e.g., "SET age = :new_age")
    ///     values: Dictionary of expression values (e.g., {":new_age": 31})
    ///     sk: Optional sort key (bytes or string)
    ///     condition: Optional condition expression
    ///
    /// Returns:
    ///     The updated item
    #[pyo3(signature = (pk, expression, values=None, sk=None, condition=None))]
    fn update(
        &self,
        py: Python<'_>,
        pk: &Bound<'_, PyAny>,
        expression: &str,
        values: Option<&Bound<'_, PyDict>>,
        sk: Option<&Bound<'_, PyAny>>,
        condition: Option<&str>,
    ) -> PyResult<PyObject> {
        let pk_bytes = extract_key(pk)?;

        let mut update = match sk {
            Some(sk_val) => {
                let sk_bytes = extract_key(sk_val)?;
                KstoneUpdate::with_sk(&pk_bytes, &sk_bytes)
            }
            None => KstoneUpdate::new(&pk_bytes)
        };

        update = update.expression(expression);

        if let Some(vals) = values {
            for (k, v) in vals.iter() {
                let key: String = k.extract()?;
                let value = py_to_value(py, &v)?;
                update = update.value(key, value);
            }
        }

        if let Some(cond) = condition {
            update = update.condition(cond);
        }

        let response = self.inner.update(update).map_err(to_py_error)?;
        Ok(item_to_dict(py, &response.item))
    }

    /// Execute a PartiQL/SQL statement.
    ///
    /// Args:
    ///     sql: SQL statement to execute
    ///
    /// Returns:
    ///     Query results or operation status
    ///
    /// Example:
    ///     >>> db.execute("SELECT * FROM items WHERE pk = 'user#123'")
    ///     >>> db.execute("INSERT INTO items VALUE {'pk': 'user#999', 'name': 'Bob'}")
    fn execute(&self, py: Python<'_>, sql: &str) -> PyResult<PyObject> {
        let response = self.inner.execute_statement(sql).map_err(to_py_error)?;

        match response {
            kstone_api::ExecuteStatementResponse::Select { items, count, .. } => {
                let dict = PyDict::new_bound(py);
                let py_items: Vec<PyObject> = items.iter()
                    .map(|item| item_to_dict(py, item))
                    .collect();
                dict.set_item("items", py_items)?;
                dict.set_item("count", count)?;
                Ok(dict.to_object(py))
            }
            kstone_api::ExecuteStatementResponse::Insert { success } => {
                let dict = PyDict::new_bound(py);
                dict.set_item("success", success)?;
                Ok(dict.to_object(py))
            }
            kstone_api::ExecuteStatementResponse::Update { item } => {
                let dict = PyDict::new_bound(py);
                dict.set_item("item", item_to_dict(py, &item))?;
                Ok(dict.to_object(py))
            }
            kstone_api::ExecuteStatementResponse::Delete { success } => {
                let dict = PyDict::new_bound(py);
                dict.set_item("success", success)?;
                Ok(dict.to_object(py))
            }
        }
    }

    /// Batch get multiple items.
    ///
    /// Args:
    ///     keys: List of partition keys to retrieve
    ///
    /// Returns:
    ///     Dictionary mapping keys to items
    fn batch_get(&self, py: Python<'_>, keys: Vec<Vec<u8>>) -> PyResult<PyObject> {
        let mut request = BatchGetRequest::new();

        for key in keys {
            request = request.add_key(&key);
        }

        let response = self.inner.batch_get(request).map_err(to_py_error)?;

        let result = PyDict::new_bound(py);
        for (key, item) in response.items {
            let key_str = String::from_utf8_lossy(&key.pk);
            result.set_item(key_str.to_string(), item_to_dict(py, &item))?;
        }
        Ok(result.to_object(py))
    }

    /// Batch write multiple items (put and/or delete).
    ///
    /// Args:
    ///     puts: Dictionary mapping keys to items to put
    ///     deletes: List of keys to delete
    ///
    /// Returns:
    ///     Number of items processed
    #[pyo3(signature = (puts=None, deletes=None))]
    fn batch_write(
        &self,
        py: Python<'_>,
        puts: Option<&Bound<'_, PyDict>>,
        deletes: Option<Vec<Vec<u8>>>,
    ) -> PyResult<usize> {
        let mut request = BatchWriteRequest::new();

        if let Some(put_items) = puts {
            for (k, v) in put_items.iter() {
                let key_bytes = extract_key(&k)?;
                let item_dict = v.downcast::<PyDict>()?;
                let item = dict_to_item(py, item_dict)?;
                request = request.put(&key_bytes, item);
            }
        }

        if let Some(delete_keys) = deletes {
            for key in delete_keys {
                request = request.delete(&key);
            }
        }

        let response = self.inner.batch_write(request).map_err(to_py_error)?;
        Ok(response.processed_count)
    }

    /// Flush pending writes to disk.
    fn flush(&self) -> PyResult<()> {
        self.inner.flush().map_err(to_py_error)
    }
}

/// Extract bytes from a Python object (bytes or string)
fn extract_key(obj: &Bound<'_, PyAny>) -> PyResult<Vec<u8>> {
    if let Ok(bytes) = obj.extract::<Vec<u8>>() {
        Ok(bytes)
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(s.into_bytes())
    } else {
        Err(PyValueError::new_err("Key must be bytes or string"))
    }
}

/// Build an item using a fluent API.
///
/// Example:
///     >>> from keystonedb import item_builder
///     >>> builder = item_builder()
///     >>> builder.string("name", "Alice")
///     >>> builder.number("age", 30)
///     >>> item = builder.build()
#[pyclass]
struct PyItemBuilder {
    inner: HashMap<String, KeystoneValue>,
}

#[pymethods]
impl PyItemBuilder {
    #[new]
    fn new() -> Self {
        PyItemBuilder {
            inner: HashMap::new(),
        }
    }

    /// Add a string attribute.
    fn string(&mut self, key: &str, value: &str) {
        self.inner.insert(key.to_string(), KeystoneValue::S(value.to_string()));
    }

    /// Add a number attribute.
    fn number(&mut self, key: &str, value: i64) {
        self.inner.insert(key.to_string(), KeystoneValue::N(value.to_string()));
    }

    /// Add a boolean attribute.
    #[pyo3(name = "bool")]
    fn bool_attr(&mut self, key: &str, value: bool) {
        self.inner.insert(key.to_string(), KeystoneValue::Bool(value));
    }

    /// Build and return the item as a dictionary.
    fn build(&self, py: Python<'_>) -> PyObject {
        item_to_dict(py, &self.inner)
    }
}

/// Create a new ItemBuilder.
#[pyfunction]
fn item_builder() -> PyItemBuilder {
    PyItemBuilder::new()
}

/// Get the version of KeystoneDB.
#[pyfunction]
fn version() -> &'static str {
    "0.1.0"
}

/// KeystoneDB Python Module
///
/// An embedded DynamoDB-compatible database for Python.
///
/// Example:
///     >>> import keystonedb
///     >>> db = keystonedb.Database.create("mydb.keystone")
///     >>> db.put(b"user#123", {"name": "Alice", "age": 30})
///     >>> print(db.get(b"user#123"))
#[pymodule]
fn keystonedb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Database>()?;
    m.add_class::<PyItemBuilder>()?;
    m.add_function(wrap_pyfunction!(item_builder, m)?)?;
    m.add_function(wrap_pyfunction!(version, m)?)?;
    Ok(())
}
