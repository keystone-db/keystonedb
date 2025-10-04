/// Test utilities and helpers for KeystoneDB testing
///
/// This module provides common test utilities to simplify writing tests.

use kstone_api::{Database, ItemBuilder, KeystoneValue};
use std::collections::HashMap;
use std::path::PathBuf;
use tempfile::TempDir;

/// Item type alias for test utilities
pub type Item = HashMap<String, KeystoneValue>;

/// Test database wrapper that manages temporary directory lifecycle
pub struct TestDatabase {
    pub db: Database,
    pub path: PathBuf,
    _temp_dir: Option<TempDir>,
}

impl TestDatabase {
    /// Create a new test database with a temporary directory
    pub fn new() -> Self {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let path = temp_dir.path().to_path_buf();
        let db = Database::create(&path).expect("Failed to create database");

        Self {
            db,
            path,
            _temp_dir: Some(temp_dir),
        }
    }

    /// Create a test database at a specific path (path must exist)
    pub fn at_path(path: PathBuf) -> Self {
        let db = Database::create(&path).expect("Failed to create database");

        Self {
            db,
            path,
            _temp_dir: None,
        }
    }

    /// Open an existing test database at a specific path
    pub fn open(path: PathBuf) -> Self {
        let db = Database::open(&path).expect("Failed to open database");

        Self {
            db,
            path,
            _temp_dir: None,
        }
    }

    /// Create an in-memory test database
    pub fn in_memory() -> Self {
        let db = Database::create_in_memory().expect("Failed to create in-memory database");

        Self {
            db,
            path: PathBuf::from(":memory:"),
            _temp_dir: None,
        }
    }

    /// Get the database path
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Flush the database
    pub fn flush(&self) {
        self.db.flush().expect("Failed to flush");
    }

    /// Close and reopen the database (for testing persistence)
    pub fn reopen(self) -> Self {
        drop(self.db);
        Self::open(self.path)
    }
}

impl Default for TestDatabase {
    fn default() -> Self {
        Self::new()
    }
}

/// Mock data generator for testing
pub struct MockDataGenerator {
    counter: u64,
}

impl MockDataGenerator {
    /// Create a new mock data generator
    pub fn new() -> Self {
        Self { counter: 0 }
    }

    /// Generate a simple item with the current counter value (does not increment)
    pub fn simple_item(&self) -> Item {
        let idx = self.counter;
        ItemBuilder::new()
            .number("index", idx as i64)
            .string("data", format!("value{}", idx))
            .build()
    }

    /// Generate an item with specified size (does not increment counter)
    pub fn sized_item(&self, bytes: usize) -> Item {
        let idx = self.counter;
        let data = "x".repeat(bytes);
        ItemBuilder::new()
            .number("index", idx as i64)
            .string("data", data)
            .build()
    }

    /// Generate a composite key (partition + sort)
    pub fn composite_key(&mut self) -> (Vec<u8>, Vec<u8>) {
        let idx = self.counter;
        self.counter += 1;
        let pk = format!("partition{}", idx / 10);
        let sk = format!("sort{}", idx % 10);
        (pk.into_bytes(), sk.into_bytes())
    }

    /// Generate a simple key
    pub fn simple_key(&mut self) -> Vec<u8> {
        let idx = self.counter;
        self.counter += 1;
        format!("key{}", idx).into_bytes()
    }
}

impl Default for MockDataGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Write batch of items to database
pub fn write_batch(db: &Database, count: usize) {
    let mut gen = MockDataGenerator::new();
    for _ in 0..count {
        let key = gen.simple_key();
        let item = gen.simple_item();
        db.put(&key, item).expect("Failed to write");
    }
}

/// Write batch of composite keys to database
pub fn write_composite_batch(db: &Database, count: usize) {
    let mut gen = MockDataGenerator::new();
    for _ in 0..count {
        let (pk, sk) = gen.composite_key();
        let item = gen.simple_item();
        db.put_with_sk(&pk, &sk, item).expect("Failed to write");
    }
}

/// Assert that a value is a number with expected value
pub fn assert_number_eq(value: &KeystoneValue, expected: &str) {
    match value {
        KeystoneValue::N(n) => assert_eq!(n, expected),
        _ => panic!("Expected number, got {:?}", value),
    }
}

/// Assert that a value is a string with expected value
pub fn assert_string_eq(value: &KeystoneValue, expected: &str) {
    match value {
        KeystoneValue::S(s) => assert_eq!(s, expected),
        _ => panic!("Expected string, got {:?}", value),
    }
}

/// Assert that a value is a boolean with expected value
pub fn assert_bool_eq(value: &KeystoneValue, expected: bool) {
    match value {
        KeystoneValue::Bool(b) => assert_eq!(*b, expected),
        _ => panic!("Expected bool, got {:?}", value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_helper() {
        let test_db = TestDatabase::new();
        let mut gen = MockDataGenerator::new();

        let key = gen.simple_key();
        let item = gen.simple_item();

        test_db.db.put(&key, item.clone()).unwrap();
        let result = test_db.db.get(&key).unwrap();
        assert_eq!(result, Some(item));
    }

    #[test]
    fn test_mock_generator() {
        let mut gen = MockDataGenerator::new();

        let key1 = gen.simple_key();
        let item1 = gen.simple_item();

        let key2 = gen.simple_key();
        let item2 = gen.simple_item();

        // Keys and items should be different
        assert_ne!(key1, key2);
        assert_ne!(item1, item2);
    }

    #[test]
    fn test_write_batch() {
        let test_db = TestDatabase::new();
        write_batch(&test_db.db, 100);

        // Verify items exist
        let result = test_db.db.get(b"key0").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_reopen() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_path_buf();

        {
            let test_db = TestDatabase::at_path(path.clone());
            write_batch(&test_db.db, 50);
            test_db.flush();
        }

        // Reopen
        let test_db2 = TestDatabase::open(path);

        // Data should persist
        let result = test_db2.db.get(b"key0").unwrap();
        assert!(result.is_some());

        let result = test_db2.db.get(b"key25").unwrap();
        assert!(result.is_some());

        let result = test_db2.db.get(b"key49").unwrap();
        assert!(result.is_some());
    }

    #[test]
    fn test_value_assertions() {
        let num = KeystoneValue::N("42".to_string());
        assert_number_eq(&num, "42");

        let str_val = KeystoneValue::S("hello".to_string());
        assert_string_eq(&str_val, "hello");

        let bool_val = KeystoneValue::Bool(true);
        assert_bool_eq(&bool_val, true);
    }
}
