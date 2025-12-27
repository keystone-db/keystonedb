//! Property-based tests using proptest
//!
//! These tests verify invariants that should hold for all inputs,
//! catching edge cases that example-based tests might miss.

use kstone_api::{Database, ItemBuilder, KeystoneValue};
use proptest::prelude::*;
use tempfile::TempDir;

// Strategy for generating valid partition keys (non-empty, printable ASCII)
fn pk_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_#]{1,50}"
}

// Strategy for generating valid sort keys
fn sk_strategy() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9_#]{1,50}"
}

// Strategy for generating string values
fn string_value_strategy() -> impl Strategy<Value = String> {
    "[ -~]{0,1000}" // Printable ASCII, 0-1000 chars
}

// Strategy for generating number values (as i64)
fn number_value_strategy() -> impl Strategy<Value = i64> {
    prop::num::i64::ANY
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // ===========================================
    // Core Invariant: Put then Get returns same value
    // ===========================================

    #[test]
    fn put_get_roundtrip_string(
        pk in pk_strategy(),
        value in string_value_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", &value).build();
        db.put(pk.as_bytes(), item.clone()).unwrap();

        let retrieved = db.get(pk.as_bytes()).unwrap();
        prop_assert!(retrieved.is_some(), "Item should exist after put");

        let retrieved = retrieved.unwrap();
        prop_assert_eq!(
            retrieved.get("data"),
            item.get("data"),
            "Retrieved value should match stored value"
        );
    }

    #[test]
    fn put_get_roundtrip_number(
        pk in pk_strategy(),
        value in number_value_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().number("count", value).build();
        db.put(pk.as_bytes(), item).unwrap();

        let retrieved = db.get(pk.as_bytes()).unwrap().unwrap();
        match retrieved.get("count") {
            Some(KeystoneValue::N(n)) => {
                let parsed: i64 = n.parse().unwrap();
                prop_assert_eq!(parsed, value);
            }
            other => prop_assert!(false, "Expected number, got {:?}", other),
        }
    }

    #[test]
    fn put_get_with_sort_key(
        pk in pk_strategy(),
        sk in sk_strategy(),
        value in string_value_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("content", &value).build();
        db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item.clone()).unwrap();

        let retrieved = db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap();
        prop_assert!(retrieved.is_some());

        let retrieved = retrieved.unwrap();
        prop_assert_eq!(retrieved.get("content"), item.get("content"));
    }

    // ===========================================
    // Invariant: Delete removes item
    // ===========================================

    #[test]
    fn delete_removes_item(pk in pk_strategy()) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", "test").build();
        db.put(pk.as_bytes(), item).unwrap();

        // Verify it exists
        prop_assert!(db.get(pk.as_bytes()).unwrap().is_some());

        // Delete it
        db.delete(pk.as_bytes()).unwrap();

        // Verify it's gone
        prop_assert!(db.get(pk.as_bytes()).unwrap().is_none());
    }

    #[test]
    fn delete_with_sort_key(
        pk in pk_strategy(),
        sk in sk_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", "test").build();
        db.put_with_sk(pk.as_bytes(), sk.as_bytes(), item).unwrap();

        // Verify it exists
        prop_assert!(db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap().is_some());

        // Delete it
        db.delete_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap();

        // Verify it's gone
        prop_assert!(db.get_with_sk(pk.as_bytes(), sk.as_bytes()).unwrap().is_none());
    }

    // ===========================================
    // Invariant: Overwrite replaces value
    // ===========================================

    #[test]
    fn overwrite_replaces_value(
        pk in pk_strategy(),
        value1 in string_value_strategy(),
        value2 in string_value_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Write first value
        let item1 = ItemBuilder::new().string("data", &value1).build();
        db.put(pk.as_bytes(), item1).unwrap();

        // Overwrite with second value
        let item2 = ItemBuilder::new().string("data", &value2).build();
        db.put(pk.as_bytes(), item2.clone()).unwrap();

        // Should have second value
        let retrieved = db.get(pk.as_bytes()).unwrap().unwrap();
        prop_assert_eq!(retrieved.get("data"), item2.get("data"));
    }

    // ===========================================
    // Invariant: Multiple items don't interfere
    // ===========================================

    #[test]
    fn multiple_items_isolated(
        pk1 in pk_strategy(),
        pk2 in pk_strategy(),
        value1 in string_value_strategy(),
        value2 in string_value_strategy()
    ) {
        // Skip if keys are the same
        prop_assume!(pk1 != pk2);

        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item1 = ItemBuilder::new().string("data", &value1).build();
        let item2 = ItemBuilder::new().string("data", &value2).build();

        db.put(pk1.as_bytes(), item1.clone()).unwrap();
        db.put(pk2.as_bytes(), item2.clone()).unwrap();

        // Each key should have its own value
        let r1 = db.get(pk1.as_bytes()).unwrap().unwrap();
        let r2 = db.get(pk2.as_bytes()).unwrap().unwrap();

        prop_assert_eq!(r1.get("data"), item1.get("data"));
        prop_assert_eq!(r2.get("data"), item2.get("data"));
    }

    // ===========================================
    // Invariant: Persistence survives reopen
    // ===========================================

    #[test]
    fn persistence_survives_reopen(
        pk in pk_strategy(),
        value in string_value_strategy()
    ) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        let item = ItemBuilder::new().string("data", &value).build();

        // Write and close
        {
            let db = Database::create(&path).unwrap();
            db.put(pk.as_bytes(), item.clone()).unwrap();
            db.flush().unwrap();
        }

        // Reopen and verify
        {
            let db = Database::open(&path).unwrap();
            let retrieved = db.get(pk.as_bytes()).unwrap();
            prop_assert!(retrieved.is_some(), "Item should persist after reopen");
            let retrieved = retrieved.unwrap();
            prop_assert_eq!(retrieved.get("data"), item.get("data"));
        }
    }

    // ===========================================
    // Invariant: Sequential writes maintain order
    // ===========================================

    #[test]
    fn sequential_overwrites_keep_latest(
        pk in pk_strategy(),
        count in 2..20usize
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Write multiple values
        for i in 0..count {
            let item = ItemBuilder::new().number("seq", i as i64).build();
            db.put(pk.as_bytes(), item).unwrap();
        }

        // Should have the last value
        let retrieved = db.get(pk.as_bytes()).unwrap().unwrap();
        match retrieved.get("seq") {
            Some(KeystoneValue::N(n)) => {
                let parsed: i64 = n.parse().unwrap();
                prop_assert_eq!(parsed, (count - 1) as i64);
            }
            other => prop_assert!(false, "Expected number, got {:?}", other),
        }
    }

    // ===========================================
    // Invariant: Delete is idempotent
    // ===========================================

    #[test]
    fn delete_is_idempotent(pk in pk_strategy()) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", "test").build();
        db.put(pk.as_bytes(), item).unwrap();

        // Delete multiple times should succeed
        db.delete(pk.as_bytes()).unwrap();
        db.delete(pk.as_bytes()).unwrap();
        db.delete(pk.as_bytes()).unwrap();

        // Should still be gone
        prop_assert!(db.get(pk.as_bytes()).unwrap().is_none());
    }

    // ===========================================
    // Invariant: Get on non-existent key returns None
    // ===========================================

    #[test]
    fn get_nonexistent_returns_none(pk in pk_strategy()) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Get without any put
        let result = db.get(pk.as_bytes()).unwrap();
        prop_assert!(result.is_none());
    }

    // ===========================================
    // Invariant: Items with same PK, different SK are independent
    // ===========================================

    #[test]
    fn sort_keys_are_independent(
        pk in pk_strategy(),
        sk1 in sk_strategy(),
        sk2 in sk_strategy(),
        value1 in string_value_strategy(),
        value2 in string_value_strategy()
    ) {
        prop_assume!(sk1 != sk2);

        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item1 = ItemBuilder::new().string("data", &value1).build();
        let item2 = ItemBuilder::new().string("data", &value2).build();

        db.put_with_sk(pk.as_bytes(), sk1.as_bytes(), item1.clone()).unwrap();
        db.put_with_sk(pk.as_bytes(), sk2.as_bytes(), item2.clone()).unwrap();

        // Each sort key should have its own value
        let r1 = db.get_with_sk(pk.as_bytes(), sk1.as_bytes()).unwrap().unwrap();
        let r2 = db.get_with_sk(pk.as_bytes(), sk2.as_bytes()).unwrap().unwrap();

        prop_assert_eq!(r1.get("data"), item1.get("data"));
        prop_assert_eq!(r2.get("data"), item2.get("data"));

        // Deleting one shouldn't affect the other
        db.delete_with_sk(pk.as_bytes(), sk1.as_bytes()).unwrap();
        prop_assert!(db.get_with_sk(pk.as_bytes(), sk1.as_bytes()).unwrap().is_none());
        prop_assert!(db.get_with_sk(pk.as_bytes(), sk2.as_bytes()).unwrap().is_some());
    }
}

// ===========================================
// Bulk property tests (more intensive)
// ===========================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(20))]

    #[test]
    fn bulk_operations_maintain_consistency(
        keys in prop::collection::vec(pk_strategy(), 10..100)
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        // Write all keys
        for (i, key) in keys.iter().enumerate() {
            let item = ItemBuilder::new().number("index", i as i64).build();
            db.put(key.as_bytes(), item).unwrap();
        }

        // Verify all keys exist (accounting for duplicates taking last value)
        let mut seen = std::collections::HashMap::new();
        for (i, key) in keys.iter().enumerate() {
            seen.insert(key.clone(), i);
        }

        for (key, expected_idx) in seen {
            let result = db.get(key.as_bytes()).unwrap();
            prop_assert!(result.is_some(), "Key {} should exist", key);

            let result = result.unwrap();
            match result.get("index") {
                Some(KeystoneValue::N(n)) => {
                    let parsed: i64 = n.parse().unwrap();
                    prop_assert_eq!(parsed, expected_idx as i64);
                }
                other => prop_assert!(false, "Expected number, got {:?}", other),
            }
        }
    }

    #[test]
    fn recovery_after_bulk_writes(
        count in 100..500usize
    ) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        // Write many items
        {
            let db = Database::create(&path).unwrap();
            for i in 0..count {
                let item = ItemBuilder::new()
                    .number("value", i as i64)
                    .string("data", format!("item_{}", i))
                    .build();
                db.put(format!("key_{}", i).as_bytes(), item).unwrap();
            }
            db.flush().unwrap();
        }

        // Reopen and verify all items
        {
            let db = Database::open(&path).unwrap();
            for i in 0..count {
                let result = db.get(format!("key_{}", i).as_bytes()).unwrap();
                prop_assert!(result.is_some(), "Key {} should exist after recovery", i);
            }
        }
    }
}

// ===========================================
// Edge case tests
// ===========================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(50))]

    #[test]
    fn handles_binary_like_keys(
        // Keys with bytes that might cause issues
        pk in prop::collection::vec(0u8..=255, 1..100)
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", "test").build();
        db.put(&pk, item.clone()).unwrap();

        let result = db.get(&pk).unwrap();
        prop_assert!(result.is_some());
        let result = result.unwrap();
        prop_assert_eq!(result.get("data"), item.get("data"));
    }

    #[test]
    fn handles_unicode_values(
        pk in pk_strategy(),
        // Unicode strings including emoji and special chars
        value in "\\PC{0,500}"
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", &value).build();

        // Should handle any valid unicode
        let put_result = db.put(pk.as_bytes(), item.clone());
        prop_assert!(put_result.is_ok());

        let get_result = db.get(pk.as_bytes()).unwrap();
        prop_assert!(get_result.is_some());
        let get_result = get_result.unwrap();
        prop_assert_eq!(get_result.get("data"), item.get("data"));
    }

    #[test]
    fn handles_empty_string_value(pk in pk_strategy()) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let item = ItemBuilder::new().string("data", "").build();
        db.put(pk.as_bytes(), item.clone()).unwrap();

        let result = db.get(pk.as_bytes()).unwrap().unwrap();
        prop_assert_eq!(result.get("data"), item.get("data"));
    }

    #[test]
    fn handles_large_values(
        pk in pk_strategy(),
        size in 1000..10000usize
    ) {
        let dir = TempDir::new().unwrap();
        let db = Database::create(dir.path()).unwrap();

        let large_value = "x".repeat(size);
        let item = ItemBuilder::new().string("data", &large_value).build();

        db.put(pk.as_bytes(), item.clone()).unwrap();

        let result = db.get(pk.as_bytes()).unwrap().unwrap();
        match result.get("data") {
            Some(KeystoneValue::S(s)) => {
                prop_assert_eq!(s.len(), size);
                prop_assert_eq!(s, &large_value);
            }
            other => prop_assert!(false, "Expected string, got {:?}", other),
        }
    }
}
