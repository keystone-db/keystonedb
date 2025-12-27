#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use kstone_api::{Database, ItemBuilder};
use tempfile::TempDir;

#[derive(Debug, Arbitrary)]
struct FuzzInput {
    key: Vec<u8>,
    sort_key: Option<Vec<u8>>,
    string_value: String,
    number_value: i64,
    operations: Vec<Operation>,
}

#[derive(Debug, Arbitrary)]
enum Operation {
    Put,
    Get,
    Delete,
}

fuzz_target!(|input: FuzzInput| {
    // Skip empty keys (invalid)
    if input.key.is_empty() {
        return;
    }

    // Limit key size to prevent OOM
    if input.key.len() > 1000 || input.sort_key.as_ref().map(|s| s.len()).unwrap_or(0) > 1000 {
        return;
    }

    // Limit string value size
    if input.string_value.len() > 10000 {
        return;
    }

    let dir = match TempDir::new() {
        Ok(d) => d,
        Err(_) => return,
    };

    let db = match Database::create(dir.path()) {
        Ok(d) => d,
        Err(_) => return,
    };

    for op in input.operations.iter().take(10) {
        match op {
            Operation::Put => {
                let item = ItemBuilder::new()
                    .string("data", &input.string_value)
                    .number("value", input.number_value)
                    .build();

                if let Some(ref sk) = input.sort_key {
                    if !sk.is_empty() {
                        let _ = db.put_with_sk(&input.key, sk, item);
                    } else {
                        let _ = db.put(&input.key, item);
                    }
                } else {
                    let _ = db.put(&input.key, item);
                }
            }
            Operation::Get => {
                if let Some(ref sk) = input.sort_key {
                    if !sk.is_empty() {
                        let _ = db.get_with_sk(&input.key, sk);
                    } else {
                        let _ = db.get(&input.key);
                    }
                } else {
                    let _ = db.get(&input.key);
                }
            }
            Operation::Delete => {
                if let Some(ref sk) = input.sort_key {
                    if !sk.is_empty() {
                        let _ = db.delete_with_sk(&input.key, sk);
                    } else {
                        let _ = db.delete(&input.key);
                    }
                } else {
                    let _ = db.delete(&input.key);
                }
            }
        }
    }
});
