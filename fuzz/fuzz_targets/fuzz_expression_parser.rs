#![no_main]

use libfuzzer_sys::fuzz_target;
use kstone_core::expression::ExpressionParser;

fuzz_target!(|data: &str| {
    // Limit input size to prevent DoS
    if data.len() > 10000 {
        return;
    }

    // The parser should never panic, only return errors
    let _ = ExpressionParser::parse(data);
});
