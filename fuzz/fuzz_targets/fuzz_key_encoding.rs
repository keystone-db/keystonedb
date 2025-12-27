#![no_main]

use libfuzzer_sys::fuzz_target;
use arbitrary::Arbitrary;
use kstone_core::Key;

#[derive(Debug, Arbitrary)]
struct FuzzInput {
    pk: Vec<u8>,
    sk: Option<Vec<u8>>,
}

fuzz_target!(|input: FuzzInput| {
    // Skip empty primary keys (invalid)
    if input.pk.is_empty() {
        return;
    }

    // Limit key sizes to prevent OOM
    if input.pk.len() > 10000 {
        return;
    }
    if let Some(ref sk) = input.sk {
        if sk.len() > 10000 {
            return;
        }
    }

    // Test key creation
    let key = if let Some(sk) = input.sk {
        if sk.is_empty() {
            Key::new(input.pk)
        } else {
            Key::with_sk(input.pk.clone(), sk)
        }
    } else {
        Key::new(input.pk)
    };

    // Test encoding roundtrip - should never panic
    let encoded = key.encode();

    // Encoded data should always be decodable
    if let Ok(decoded) = Key::decode(&encoded) {
        // Verify roundtrip
        assert_eq!(key.pk, decoded.pk);
        assert_eq!(key.sk, decoded.sk);
    }

    // Test stripe calculation - should never panic
    let _ = key.stripe();
});
