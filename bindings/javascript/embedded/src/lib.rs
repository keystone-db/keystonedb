#![deny(clippy::all)]

use kstone_api::Database as KstoneDatabase;
use kstone_core::Value;
use napi::bindgen_prelude::*;
use napi::JsString;
use napi_derive::napi;
use std::collections::HashMap;

/// KeystoneDB Database
#[napi]
pub struct Database {
    inner: KstoneDatabase,
}

impl Database {
    fn map_error(err: kstone_core::Error) -> napi::Error {
        napi::Error::from_reason(err.to_string())
    }

    fn js_to_value(env: &Env, value: Unknown) -> Result<Value> {
        match value.get_type()? {
            ValueType::String => {
                let s: String = value.coerce_to_string()?.into_utf8()?.as_str()?.to_string();
                Ok(Value::S(s))
            }
            ValueType::Number => {
                let n: f64 = value.coerce_to_number()?.get_double()?;
                Ok(Value::N(n.to_string()))
            }
            ValueType::Boolean => {
                let b: bool = value.coerce_to_bool()?.get_value()?;
                Ok(Value::Bool(b))
            }
            ValueType::Object => {
                let obj = value.coerce_to_object()?;

                // Check if it's a Buffer
                if obj.is_buffer()? {
                    let buffer_ref = obj.as_ref(env);
                    let buffer_val = unsafe { buffer_ref.cast::<Buffer>() };
                    Ok(Value::B(buffer_val.to_vec().into()))
                }
                // Check if it's an Array
                else if obj.is_array()? {
                    let arr: Object = obj;
                    let len = arr.get_array_length()?;
                    let mut values = Vec::new();

                    for i in 0..len {
                        let item: Unknown = arr.get_element(i)?;
                        values.push(Self::js_to_value(env, item)?);
                    }

                    Ok(Value::L(values))
                }
                // Otherwise treat as a Map
                else {
                    let obj_ref: Object = obj;
                    let keys = obj_ref.get_property_names()?;
                    let keys_len = keys.get_array_length()?;
                    let mut map = HashMap::new();

                    for i in 0..keys_len {
                        let key: JsString = keys.get_element(i)?;
                        let key_str = key.into_utf8()?.as_str()?.to_string();
                        let val: Unknown = obj_ref.get_named_property(&key_str)?;
                        map.insert(key_str, Self::js_to_value(env, val)?);
                    }

                    Ok(Value::M(map))
                }
            }
            ValueType::Null | ValueType::Undefined => Ok(Value::Null),
            _ => Err(napi::Error::from_reason(format!(
                "Unsupported type: {:?}",
                value.get_type()?
            ))),
        }
    }

    fn value_to_js(env: &Env, value: &Value) -> Result<Unknown> {
        match value {
            Value::S(s) => env.create_string(s).map(|v| v.into_unknown()),
            Value::N(n) => {
                // Try to parse as int first, then float
                if let Ok(i) = n.parse::<i64>() {
                    env.create_int64(i).map(|v| v.into_unknown())
                } else if let Ok(f) = n.parse::<f64>() {
                    env.create_double(f).map(|v| v.into_unknown())
                } else {
                    env.create_string(n).map(|v| v.into_unknown())
                }
            }
            Value::B(b) => env
                .create_buffer_copy(b.as_ref())
                .map(|v| v.into_unknown()),
            Value::Bool(b) => env.get_boolean(*b).map(|v| v.into_unknown()),
            Value::Null => env.get_null().map(|v| v.into_unknown()),
            Value::L(list) => {
                let mut arr = env.create_array(list.len() as u32)?;
                for (i, item) in list.iter().enumerate() {
                    arr.set(i as u32, Self::value_to_js(env, item)?)?;
                }
                arr.coerce_to_object().map(|o| o.into_unknown())
            }
            Value::M(map) => {
                let mut obj = env.create_object()?;
                for (key, value) in map {
                    obj.set_named_property(key, Self::value_to_js(env, value)?)?;
                }
                Ok(obj.into_unknown())
            }
            Value::VecF32(vec) => {
                let mut arr = env.create_array(vec.len() as u32)?;
                for (i, &f) in vec.iter().enumerate() {
                    arr.set(i as u32, env.create_double(f as f64)?)?;
                }
                arr.coerce_to_object().map(|o| o.into_unknown())
            }
            Value::Ts(ts) => env.create_int64(*ts).map(|v| v.into_unknown()),
        }
    }
}

#[napi]
impl Database {
    /// Create a new database at the specified path
    #[napi(factory)]
    pub fn create(path: String) -> Result<Self> {
        let db = KstoneDatabase::create(&path).map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Open an existing database at the specified path
    #[napi(factory)]
    pub fn open(path: String) -> Result<Self> {
        let db = KstoneDatabase::open(&path).map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Create an in-memory database
    #[napi(factory)]
    pub fn create_in_memory() -> Result<Self> {
        let db = KstoneDatabase::create_in_memory().map_err(Self::map_error)?;
        Ok(Database { inner: db })
    }

    /// Put an item into the database
    #[napi]
    pub fn put(&mut self, env: Env, pk: Buffer, item: Object) -> Result<()> {
        let mut ks_item = HashMap::new();

        let keys = item.get_property_names()?;
        let keys_len = keys.get_array_length()?;

        for i in 0..keys_len {
            let key: JsString = keys.get_element(i)?;
            let key_str = key.into_utf8()?.as_str()?.to_string();
            let value: Unknown = item.get_named_property(&key_str)?;
            ks_item.insert(key_str, Self::js_to_value(&env, value)?);
        }

        self.inner.put(pk.as_ref(), ks_item).map_err(Self::map_error)
    }

    /// Put an item with sort key
    #[napi]
    pub fn put_with_sk(&mut self, env: Env, pk: Buffer, sk: Buffer, item: Object) -> Result<()> {
        let mut ks_item = HashMap::new();

        let keys = item.get_property_names()?;
        let keys_len = keys.get_array_length()?;

        for i in 0..keys_len {
            let key: JsString = keys.get_element(i)?;
            let key_str = key.into_utf8()?.as_str()?.to_string();
            let value: Unknown = item.get_named_property(&key_str)?;
            ks_item.insert(key_str, Self::js_to_value(&env, value)?);
        }

        self.inner
            .put_with_sk(pk.as_ref(), sk.as_ref(), ks_item)
            .map_err(Self::map_error)
    }

    /// Get an item from the database
    #[napi]
    pub fn get(&self, env: Env, pk: Buffer) -> Result<Option<Object>> {
        match self.inner.get(pk.as_ref()).map_err(Self::map_error)? {
            Some(item) => {
                let mut obj = env.create_object()?;
                for (key, value) in &item {
                    obj.set_named_property(key, Self::value_to_js(&env, value)?)?;
                }
                Ok(Some(obj))
            }
            None => Ok(None),
        }
    }

    /// Get an item with sort key
    #[napi]
    pub fn get_with_sk(&self, env: Env, pk: Buffer, sk: Buffer) -> Result<Option<Object>> {
        match self
            .inner
            .get_with_sk(pk.as_ref(), sk.as_ref())
            .map_err(Self::map_error)?
        {
            Some(item) => {
                let mut obj = env.create_object()?;
                for (key, value) in &item {
                    obj.set_named_property(key, Self::value_to_js(&env, value)?)?;
                }
                Ok(Some(obj))
            }
            None => Ok(None),
        }
    }

    /// Delete an item from the database
    #[napi]
    pub fn delete(&mut self, pk: Buffer) -> Result<()> {
        self.inner.delete(pk.as_ref()).map_err(Self::map_error)
    }

    /// Delete an item with sort key
    #[napi]
    pub fn delete_with_sk(&mut self, pk: Buffer, sk: Buffer) -> Result<()> {
        self.inner
            .delete_with_sk(pk.as_ref(), sk.as_ref())
            .map_err(Self::map_error)
    }

    /// Flush database to disk
    #[napi]
    pub fn flush(&mut self) -> Result<()> {
        self.inner.flush().map_err(Self::map_error)
    }
}
