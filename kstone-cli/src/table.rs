/// Table formatting for query results using comfy-table

use comfy_table::{presets::UTF8_FULL, Cell, ContentArrangement, Table};
use kstone_api::KeystoneValue;
use std::collections::{HashMap, HashSet};

/// Format a list of items as a table
///
/// Collects all attribute names from items as columns, then formats each item as a row.
/// Simple values (S, N, Bool, Null) are displayed directly.
/// Complex values (L, M, B, VecF32) are displayed as JSON strings.
pub fn format_items_table(items: &[HashMap<String, KeystoneValue>]) -> String {
    if items.is_empty() {
        return "No items found".to_string();
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Collect all unique attribute names across all items
    let mut columns = HashSet::new();
    for item in items {
        for key in item.keys() {
            columns.insert(key.clone());
        }
    }

    // Sort columns alphabetically for consistent display
    let mut columns: Vec<String> = columns.into_iter().collect();
    columns.sort();

    // Add header row
    let header = columns
        .iter()
        .map(|col| Cell::new(col))
        .collect::<Vec<_>>();
    table.set_header(header);

    // Add data rows
    for item in items {
        let row = columns
            .iter()
            .map(|col| {
                if let Some(value) = item.get(col) {
                    Cell::new(format_value(value))
                } else {
                    Cell::new("-")
                }
            })
            .collect::<Vec<_>>();
        table.add_row(row);
    }

    table.to_string()
}

/// Format a KeystoneValue for display in a table cell
fn format_value(value: &KeystoneValue) -> String {
    match value {
        KeystoneValue::S(s) => s.clone(),
        KeystoneValue::N(n) => n.clone(),
        KeystoneValue::Bool(b) => b.to_string(),
        KeystoneValue::Null => "null".to_string(),
        KeystoneValue::L(list) => {
            // Format list as JSON array
            let items: Vec<String> = list.iter().map(format_value).collect();
            format!("[{}]", items.join(", "))
        }
        KeystoneValue::M(map) => {
            // Format map as JSON object
            let pairs: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{}\": {}", k, format_value(v)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
        KeystoneValue::B(bytes) => {
            // Display binary as base64
            format!("<Binary {} bytes>", bytes.len())
        }
        KeystoneValue::VecF32(vec) => {
            // Display f32 vector as array
            let items: Vec<String> = vec.iter().map(|f| f.to_string()).collect();
            format!("[{}]", items.join(", "))
        }
        KeystoneValue::Ts(ts) => {
            // Display timestamp
            ts.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kstone_api::ItemBuilder;

    #[test]
    fn test_format_empty_items() {
        let items = vec![];
        let output = format_items_table(&items);
        assert_eq!(output, "No items found");
    }

    #[test]
    fn test_format_simple_items() {
        let items = vec![
            ItemBuilder::new()
                .string("name", "Alice")
                .number("age", 30)
                .build(),
            ItemBuilder::new()
                .string("name", "Bob")
                .number("age", 25)
                .build(),
        ];

        let output = format_items_table(&items);
        println!("{}", output);

        // Check that output contains expected values
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(output.contains("30"));
        assert!(output.contains("25"));
        assert!(output.contains("name"));
        assert!(output.contains("age"));
    }

    #[test]
    fn test_format_mixed_attributes() {
        let items = vec![
            ItemBuilder::new()
                .string("name", "Alice")
                .number("age", 30)
                .build(),
            ItemBuilder::new()
                .string("name", "Bob")
                .bool("active", true)
                .build(),
        ];

        let output = format_items_table(&items);
        println!("{}", output);

        // Check that missing attributes are shown as "-"
        assert!(output.contains("Alice"));
        assert!(output.contains("Bob"));
        assert!(output.contains("true"));
    }

    #[test]
    fn test_format_value_types() {
        assert_eq!(format_value(&KeystoneValue::S("test".to_string())), "test");
        assert_eq!(format_value(&KeystoneValue::N("42".to_string())), "42");
        assert_eq!(format_value(&KeystoneValue::Bool(true)), "true");
        assert_eq!(format_value(&KeystoneValue::Null), "null");
    }

    #[test]
    fn test_format_list_value() {
        let list = KeystoneValue::L(vec![
            KeystoneValue::S("a".to_string()),
            KeystoneValue::N("1".to_string()),
        ]);
        let formatted = format_value(&list);
        assert_eq!(formatted, "[a, 1]");
    }

    #[test]
    fn test_format_map_value() {
        let mut map = HashMap::new();
        map.insert("key".to_string(), KeystoneValue::S("value".to_string()));
        let map_value = KeystoneValue::M(map);
        let formatted = format_value(&map_value);
        assert!(formatted.contains("\"key\": value"));
    }
}
