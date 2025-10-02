/// Type conversions between protobuf and KeystoneDB types
///
/// This module provides bidirectional conversions for all data types
/// used in the gRPC protocol and KeystoneDB internal representation.
///
/// Due to Rust's orphan rules, we use conversion functions instead of
/// trait implementations.

use bytes::Bytes;
use kstone_core::{Key, Value as KsValue};
use kstone_proto::{self as proto, value::Value as ProtoValueEnum};
use std::collections::HashMap;
use tonic::Status;

pub type Item = HashMap<String, KsValue>;

// ============================================================================
// Value Conversions
// ============================================================================

/// Convert protobuf Value to KeystoneDB Value
pub fn proto_value_to_ks(value: proto::Value) -> Result<KsValue, Status> {
    let value_enum = value
        .value
        .ok_or_else(|| Status::invalid_argument("Value field is missing"))?;

    match value_enum {
        ProtoValueEnum::StringValue(s) => Ok(KsValue::S(s)),
        ProtoValueEnum::NumberValue(n) => Ok(KsValue::N(n)),
        ProtoValueEnum::BinaryValue(b) => Ok(KsValue::B(Bytes::from(b))),
        ProtoValueEnum::BoolValue(b) => Ok(KsValue::Bool(b)),
        ProtoValueEnum::NullValue(_) => Ok(KsValue::Null),
        ProtoValueEnum::ListValue(list) => {
            let items: Result<Vec<KsValue>, Status> =
                list.items.into_iter().map(proto_value_to_ks).collect();
            Ok(KsValue::L(items?))
        }
        ProtoValueEnum::MapValue(map) => {
            let mut kv_map = HashMap::new();
            for (k, v) in map.fields {
                kv_map.insert(k, proto_value_to_ks(v)?);
            }
            Ok(KsValue::M(kv_map))
        }
        ProtoValueEnum::VectorValue(vec) => Ok(KsValue::VecF32(vec.values)),
        ProtoValueEnum::TimestampValue(ts) => Ok(KsValue::Ts(ts as i64)),
    }
}

/// Convert KeystoneDB Value to protobuf Value
pub fn ks_value_to_proto(value: &KsValue) -> proto::Value {
    let value_enum = match value {
        KsValue::S(s) => ProtoValueEnum::StringValue(s.clone()),
        KsValue::N(n) => ProtoValueEnum::NumberValue(n.clone()),
        KsValue::B(b) => ProtoValueEnum::BinaryValue(b.to_vec()),
        KsValue::Bool(b) => ProtoValueEnum::BoolValue(*b),
        KsValue::Null => ProtoValueEnum::NullValue(proto::NullValue::NullValue as i32),
        KsValue::L(items) => {
            let proto_items: Vec<proto::Value> = items.iter().map(ks_value_to_proto).collect();
            ProtoValueEnum::ListValue(proto::ListValue { items: proto_items })
        }
        KsValue::M(map) => {
            let mut proto_map = HashMap::new();
            for (k, v) in map {
                proto_map.insert(k.clone(), ks_value_to_proto(v));
            }
            ProtoValueEnum::MapValue(proto::MapValue { fields: proto_map })
        }
        KsValue::VecF32(vec) => ProtoValueEnum::VectorValue(proto::VectorValue {
            values: vec.clone(),
        }),
        KsValue::Ts(ts) => ProtoValueEnum::TimestampValue(*ts as u64),
    };

    proto::Value {
        value: Some(value_enum),
    }
}

// ============================================================================
// Item Conversions
// ============================================================================

/// Convert protobuf Item to KeystoneDB Item (HashMap)
pub fn proto_item_to_ks(item: proto::Item) -> Result<Item, Status> {
    let mut kv_map = HashMap::new();
    for (k, v) in item.attributes {
        kv_map.insert(k, proto_value_to_ks(v)?);
    }
    Ok(kv_map)
}

/// Convert KeystoneDB Item to protobuf Item
pub fn ks_item_to_proto(item: &Item) -> proto::Item {
    let mut attributes = HashMap::new();
    for (k, v) in item {
        attributes.insert(k.clone(), ks_value_to_proto(v));
    }
    proto::Item { attributes }
}

// ============================================================================
// Key Conversions
// ============================================================================

/// Convert protobuf Key to (partition_key, optional sort_key)
pub fn proto_key_to_ks(key: proto::Key) -> (Bytes, Option<Bytes>) {
    let pk = Bytes::from(key.partition_key);
    let sk = key.sort_key.map(Bytes::from);
    (pk, sk)
}

/// Convert (partition_key, optional sort_key) to protobuf Key
pub fn ks_key_to_proto(pk: impl Into<Vec<u8>>, sk: Option<impl Into<Vec<u8>>>) -> proto::Key {
    proto::Key {
        partition_key: pk.into(),
        sort_key: sk.map(|s| s.into()),
    }
}

/// Convert protobuf Key to kstone_core Key
pub fn proto_key_to_core_key(key: proto::Key) -> Key {
    if let Some(sk) = key.sort_key {
        Key::with_sk(Bytes::from(key.partition_key), Bytes::from(sk))
    } else {
        Key::new(Bytes::from(key.partition_key))
    }
}

/// Convert kstone_core Key to protobuf Key
pub fn core_key_to_proto(key: &Key) -> proto::Key {
    proto::Key {
        partition_key: key.pk.to_vec(),
        sort_key: key.sk.as_ref().map(|s| s.to_vec()),
    }
}

// ============================================================================
// LastKey Conversions
// ============================================================================

/// Convert protobuf LastKey to (partition_key, optional sort_key)
pub fn proto_last_key_to_ks(key: proto::LastKey) -> (Bytes, Option<Bytes>) {
    let pk = Bytes::from(key.partition_key);
    let sk = key.sort_key.map(Bytes::from);
    (pk, sk)
}

/// Convert (partition_key, optional sort_key) to protobuf LastKey
pub fn ks_last_key_to_proto(
    pk: impl Into<Vec<u8>>,
    sk: Option<impl Into<Vec<u8>>>,
) -> proto::LastKey {
    proto::LastKey {
        partition_key: pk.into(),
        sort_key: sk.map(|s| s.into()),
    }
}

// ============================================================================
// Helper Functions for Option<LastKey>
// ============================================================================

/// Convert Option<(Bytes, Option<Bytes>)> to Option<proto::LastKey>
pub fn ks_last_key_opt_to_proto(last_key: Option<(Bytes, Option<Bytes>)>) -> Option<proto::LastKey> {
    last_key.map(|(pk, sk)| ks_last_key_to_proto(pk, sk))
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_string_roundtrip() {
        let ks_value = KsValue::S("hello".to_string());
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_number_roundtrip() {
        let ks_value = KsValue::N("42.5".to_string());
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_bool_roundtrip() {
        let ks_value = KsValue::Bool(true);
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_null_roundtrip() {
        let ks_value = KsValue::Null;
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_binary_roundtrip() {
        let ks_value = KsValue::B(Bytes::from(vec![1, 2, 3, 4]));
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_list_roundtrip() {
        let ks_value = KsValue::L(vec![
            KsValue::S("a".to_string()),
            KsValue::N("123".to_string()),
            KsValue::Bool(false),
        ]);
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_map_roundtrip() {
        let mut map = HashMap::new();
        map.insert("name".to_string(), KsValue::S("Alice".to_string()));
        map.insert("age".to_string(), KsValue::N("30".to_string()));

        let ks_value = KsValue::M(map);
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_vector_roundtrip() {
        let ks_value = KsValue::VecF32(vec![1.0, 2.0, 3.0]);
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_value_timestamp_roundtrip() {
        let ks_value = KsValue::Ts(1234567890);
        let proto_value = ks_value_to_proto(&ks_value);
        let converted = proto_value_to_ks(proto_value).unwrap();
        assert_eq!(ks_value, converted);
    }

    #[test]
    fn test_item_roundtrip() {
        let mut item = HashMap::new();
        item.insert("name".to_string(), KsValue::S("Alice".to_string()));
        item.insert("age".to_string(), KsValue::N("30".to_string()));
        item.insert("active".to_string(), KsValue::Bool(true));

        let proto_item = ks_item_to_proto(&item);
        let converted = proto_item_to_ks(proto_item).unwrap();
        assert_eq!(item, converted);
    }

    #[test]
    fn test_key_with_sort_key() {
        let proto_key = ks_key_to_proto(b"pk123".to_vec(), Some(b"sk456".to_vec()));
        let (pk, sk) = proto_key_to_ks(proto_key);
        assert_eq!(pk, Bytes::from("pk123"));
        assert_eq!(sk, Some(Bytes::from("sk456")));
    }

    #[test]
    fn test_key_without_sort_key() {
        let proto_key = ks_key_to_proto(b"pk123".to_vec(), None::<Vec<u8>>);
        let (pk, sk) = proto_key_to_ks(proto_key);
        assert_eq!(pk, Bytes::from("pk123"));
        assert_eq!(sk, None);
    }

    #[test]
    fn test_empty_item() {
        let item = HashMap::new();
        let proto_item = ks_item_to_proto(&item);
        let converted = proto_item_to_ks(proto_item).unwrap();
        assert_eq!(item, converted);
    }

    #[test]
    fn test_nested_map() {
        let mut inner_map = HashMap::new();
        inner_map.insert("city".to_string(), KsValue::S("NYC".to_string()));

        let mut item = HashMap::new();
        item.insert("name".to_string(), KsValue::S("Alice".to_string()));
        item.insert("address".to_string(), KsValue::M(inner_map));

        let proto_item = ks_item_to_proto(&item);
        let converted = proto_item_to_ks(proto_item).unwrap();
        assert_eq!(item, converted);
    }
}
