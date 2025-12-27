#![no_main]

use libfuzzer_sys::fuzz_target;
use kstone_core::partiql::{Parser, Validator, Translator};

fuzz_target!(|data: &str| {
    // Limit input size to prevent DoS
    if data.len() > 10000 {
        return;
    }

    // Skip empty or whitespace-only input
    if data.trim().is_empty() {
        return;
    }

    // Try to parse - should never panic
    let parse_result = Parser::parse(data);

    // If parsing succeeded, try validation
    if let Ok(statement) = parse_result {
        // Validation should never panic
        let _ = Validator::validate(&statement);

        // Translation should never panic
        let _ = Translator::translate(&statement);
    }
});
