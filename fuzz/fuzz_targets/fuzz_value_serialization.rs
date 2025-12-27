#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use kstone_core::Value;
use std::collections::HashMap;

#[derive(Debug, Arbitrary)]
enum FuzzValue {
    String(String),
    Number(String),
    Bool(bool),
    Null,
    Binary(Vec<u8>),
    List(Vec<FuzzValue>),
    Map(Vec<(String, FuzzValue)>),
    Timestamp(i64),
    Vector(Vec<f32>),
}

impl FuzzValue {
    fn to_value(&self) -> Value {
        match self {
            FuzzValue::String(s) => Value::S(s.clone()),
            FuzzValue::Number(n) => Value::N(n.clone()),
            FuzzValue::Bool(b) => Value::Bool(*b),
            FuzzValue::Null => Value::Null,
            FuzzValue::Binary(b) => Value::B(bytes::Bytes::from(b.clone())),
            FuzzValue::List(l) => {
                // Limit depth to prevent stack overflow
                if l.len() > 100 {
                    Value::L(l.iter().take(100).map(|v| v.to_value()).collect())
                } else {
                    Value::L(l.iter().map(|v| v.to_value()).collect())
                }
            }
            FuzzValue::Map(m) => {
                let mut map = HashMap::new();
                for (k, v) in m.iter().take(100) {
                    map.insert(k.clone(), v.to_value());
                }
                Value::M(map)
            }
            FuzzValue::Timestamp(t) => Value::Ts(*t),
            FuzzValue::Vector(v) => Value::VecF32(v.clone()),
        }
    }
}

fuzz_target!(|input: FuzzValue| {
    // Convert to Value
    let value = input.to_value();

    // Test Debug formatting - should never panic
    let _ = format!("{:?}", value);

    // Test clone - should never panic
    let cloned = value.clone();

    // Test equality - should never panic
    let _ = value == cloned;
});
