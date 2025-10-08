use kstone_api::Database as KstoneDatabase;
use kstone_core::{Item as KstoneItem, Value};
use pyo3::exceptions::{PyIOError, PyKeyError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyDict, PyFloat, PyList, PyString};
use std::collections::HashMap;

/// KeystoneDB Database
#[pyclass]
struct Database {
    inner: KstoneDatabase,
}

/// KeystoneDB Item
#[pyclass]
struct Item {
    inner: KstoneItem,
}

impl Database {
    fn map_error(err: kstone_core::Error) -> PyErr {
        match err {
            kstone_core::Error::NotFound(_) => PyKeyError::new_err(err.to_string()),
            kstone_core::Error::Io(e) => PyIOError::new_err(e.to_string()),
            kstone_core::Error::InvalidArgument(msg) => PyValueError::new_err(msg),
            kstone_core::Error::InvalidQuery(msg) => PyValueError::new_err(msg),
            _ => PyRuntimeError::new_err(err.to_string()),
        }
    }

    fn py_to_value(obj: &Bound<'_, PyAny>) -> PyResult<Value> {
        if obj.is_instance_of::<PyString>() {
            let s: String = obj.extract()?;
            Ok(Value::S(s))
        } else if obj.is_instance_of::<PyBool>() {
            let b: bool = obj.extract()?;
            Ok(Value::Bool(b))
        } else if obj.is_instance_of::<PyFloat>() {
            let f: f64 = obj.extract()?;
            Ok(Value::N(f.to_string()))
        } else if let Ok(i) = obj.extract::<i64>() {
            Ok(Value::N(i.to_string()))
        } else if obj.is_instance_of::<PyBytes>() {
            let bytes: Vec<u8> = obj.extract()?;
            Ok(Value::B(bytes.into()))
        } else if obj.is_instance_of::<PyList>() {
            let list: &Bound<'_, PyList> = obj.downcast()?;
            let mut values = Vec::new();
            for item in list.iter() {
                values.push(Self::py_to_value(&item)?);
            }
            Ok(Value::L(values))
        } else if obj.is_instance_of::<PyDict>() {
            let dict: &Bound<'_, PyDict> = obj.downcast()?;
            let mut map = HashMap::new();
            for (key, value) in dict.iter() {
                let key_str: String = key.extract()?;
                let val = Self::py_to_value(&value)?;
                map.insert(key_str, val);
            }
            Ok(Value::M(map))
        } else if obj.is_none() {
            Ok(Value::Null)
        } else {
            Err(PyValueError::new_err(format!(
                "Unsupported type: {}",
                obj.get_type().name()?
            )))
        }
    }

    fn value_to_py(py: Python, value: &Value) -> PyResult<PyObject> {
        match value {
            Value::S(s) => Ok(s.to_object(py)),
            Value::N(n) => {
                // Try to parse as int first, then float
                if let Ok(i) = n.parse::<i64>() {
                    Ok(i.to_object(py))
                } else if let Ok(f) = n.parse::<f64>() {
                    Ok(f.to_object(py))
                } else {
                    Ok(n.to_object(py)) // Return as string if parsing fails
                }
            }
            Value::B(b) => Ok(PyBytes::new_bound(py, b.as_ref()).to_object(py)),
            Value::Bool(b) => Ok(b.to_object(py)),
            Value::Null => Ok(py.None()),
            Value::L(list) => {
                let py_list = PyList::empty_bound(py);
                for item in list {
                    py_list.append(Self::value_to_py(py, item)?)?;
                }
                Ok(py_list.to_object(py))
            }
            Value::M(map) => {
                let py_dict = PyDict::new_bound(py);
                for (key, value) in map {
                    py_dict.set_item(key, Self::value_to_py(py, value)?)?;
                }
                Ok(py_dict.to_object(py))
            }
            Value::VecF32(vec) => {
                let py_list = PyList::empty_bound(py);
                for &f in vec {
                    py_list.append(f)?;
                }
                Ok(py_list.to_object(py))
            }
            Value::Ts(ts) => Ok(ts.to_object(py)),
        }
    }
}

#[pymethods]
impl Database {
    /// Create a new database at the specified path
    #[staticmethod]
    fn create(path: &str) -> PyResult<Self> {
        let db = KstoneDatabase::create(path).map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Open an existing database at the specified path
    #[staticmethod]
    fn open(path: &str) -> PyResult<Self> {
        let db = KstoneDatabase::open(path).map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Create an in-memory database
    #[staticmethod]
    fn create_in_memory() -> PyResult<Self> {
        let db = KstoneDatabase::create_in_memory().map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Put an item into the database
    fn put(&mut self, pk: &[u8], item: &Bound<'_, PyDict>) -> PyResult<()> {
        let mut ks_item = HashMap::new();

        for (key, value) in item.iter() {
            let key_str: String = key.extract()?;
            let val = Self::py_to_value(&value)?;
            ks_item.insert(key_str, val);
        }

        self.inner.put(pk, ks_item).map_err(Self::map_error)
    }

    /// Put an item with sort key
    fn put_with_sk(&mut self, pk: &[u8], sk: &[u8], item: &Bound<'_, PyDict>) -> PyResult<()> {
        let mut ks_item = HashMap::new();

        for (key, value) in item.iter() {
            let key_str: String = key.extract()?;
            let val = Self::py_to_value(&value)?;
            ks_item.insert(key_str, val);
        }

        self.inner
            .put_with_sk(pk, sk, ks_item)
            .map_err(Self::map_error)
    }

    /// Get an item from the database
    fn get(&self, py: Python, pk: &[u8]) -> PyResult<Option<PyObject>> {
        match self.inner.get(pk).map_err(Self::map_error)? {
            Some(item) => {
                let py_dict = PyDict::new_bound(py);
                for (key, value) in &item {
                    py_dict.set_item(key, Self::value_to_py(py, value)?)?;
                }
                Ok(Some(py_dict.to_object(py)))
            }
            None => Ok(None),
        }
    }

    /// Get an item with sort key
    fn get_with_sk(&self, py: Python, pk: &[u8], sk: &[u8]) -> PyResult<Option<PyObject>> {
        match self.inner.get_with_sk(pk, sk).map_err(Self::map_error)? {
            Some(item) => {
                let py_dict = PyDict::new_bound(py);
                for (key, value) in &item {
                    py_dict.set_item(key, Self::value_to_py(py, value)?)?;
                }
                Ok(Some(py_dict.to_object(py)))
            }
            None => Ok(None),
        }
    }

    /// Delete an item from the database
    fn delete(&mut self, pk: &[u8]) -> PyResult<()> {
        self.inner.delete(pk).map_err(Self::map_error)
    }

    /// Delete an item with sort key
    fn delete_with_sk(&mut self, pk: &[u8], sk: &[u8]) -> PyResult<()> {
        self.inner.delete_with_sk(pk, sk).map_err(Self::map_error)
    }

    /// Flush database to disk
    fn flush(&mut self) -> PyResult<()> {
        self.inner.flush().map_err(Self::map_error)
    }
}

/// KeystoneDB Python module
#[pymodule]
fn keystonedb(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Database>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
