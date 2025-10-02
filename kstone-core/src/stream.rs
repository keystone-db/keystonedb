/// Stream support for Change Data Capture (Phase 3.4)
///
/// Provides DynamoDB-style streams that capture item-level modifications
/// (INSERT, MODIFY, REMOVE) with configurable view types.

use crate::{Key, Item};
use serde::{Deserialize, Serialize};

/// Stream view type - controls what data is included in stream records
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamViewType {
    /// Only the key attributes of the item
    KeysOnly,
    /// The entire item as it appears after modification
    NewImage,
    /// The entire item as it appeared before modification
    OldImage,
    /// Both the new and old images
    NewAndOldImages,
}

impl Default for StreamViewType {
    fn default() -> Self {
        StreamViewType::NewAndOldImages
    }
}

/// Stream event type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamEventType {
    /// A new item was added to the table
    Insert,
    /// An existing item was updated
    Modify,
    /// An item was deleted from the table
    Remove,
}

/// A stream record capturing a single item change
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamRecord {
    /// Sequence number (globally unique, monotonically increasing)
    pub sequence_number: u64,
    /// Type of change event
    pub event_type: StreamEventType,
    /// The key of the modified item
    pub key: Key,
    /// The item before modification (if applicable and view type includes it)
    pub old_image: Option<Item>,
    /// The item after modification (if applicable and view type includes it)
    pub new_image: Option<Item>,
    /// Approximate timestamp when the change occurred (milliseconds since epoch)
    pub timestamp: i64,
}

impl StreamRecord {
    /// Create an INSERT record
    pub fn insert(sequence_number: u64, key: Key, new_image: Item, view_type: StreamViewType) -> Self {
        let (old, new) = match view_type {
            StreamViewType::KeysOnly => (None, None),
            StreamViewType::NewImage | StreamViewType::NewAndOldImages => (None, Some(new_image)),
            StreamViewType::OldImage => (None, None), // No old image for insert
        };

        Self {
            sequence_number,
            event_type: StreamEventType::Insert,
            key,
            old_image: old,
            new_image: new,
            timestamp: current_timestamp_millis(),
        }
    }

    /// Create a MODIFY record
    pub fn modify(
        sequence_number: u64,
        key: Key,
        old_image: Item,
        new_image: Item,
        view_type: StreamViewType,
    ) -> Self {
        let (old, new) = match view_type {
            StreamViewType::KeysOnly => (None, None),
            StreamViewType::NewImage => (None, Some(new_image)),
            StreamViewType::OldImage => (Some(old_image), None),
            StreamViewType::NewAndOldImages => (Some(old_image), Some(new_image)),
        };

        Self {
            sequence_number,
            event_type: StreamEventType::Modify,
            key,
            old_image: old,
            new_image: new,
            timestamp: current_timestamp_millis(),
        }
    }

    /// Create a REMOVE record
    pub fn remove(sequence_number: u64, key: Key, old_image: Item, view_type: StreamViewType) -> Self {
        let (old, new) = match view_type {
            StreamViewType::KeysOnly => (None, None),
            StreamViewType::OldImage | StreamViewType::NewAndOldImages => (Some(old_image), None),
            StreamViewType::NewImage => (None, None), // No new image for remove
        };

        Self {
            sequence_number,
            event_type: StreamEventType::Remove,
            key,
            old_image: old,
            new_image: new,
            timestamp: current_timestamp_millis(),
        }
    }
}

/// Stream configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamConfig {
    /// Whether streams are enabled
    pub enabled: bool,
    /// What data to include in stream records
    pub view_type: StreamViewType,
    /// Maximum number of records to retain in the stream buffer
    /// (oldest records are dropped when buffer is full)
    pub buffer_size: usize,
}

impl Default for StreamConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            view_type: StreamViewType::NewAndOldImages,
            buffer_size: 1000,
        }
    }
}

impl StreamConfig {
    /// Create a new stream config with streams enabled
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Default::default()
        }
    }

    /// Set the view type
    pub fn with_view_type(mut self, view_type: StreamViewType) -> Self {
        self.view_type = view_type;
        self
    }

    /// Set the buffer size
    pub fn with_buffer_size(mut self, size: usize) -> Self {
        self.buffer_size = size;
        self
    }
}

/// Get current timestamp in milliseconds since epoch
fn current_timestamp_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Value;
    use std::collections::HashMap;

    #[test]
    fn test_stream_view_type_default() {
        assert_eq!(StreamViewType::default(), StreamViewType::NewAndOldImages);
    }

    #[test]
    fn test_stream_config_default() {
        let config = StreamConfig::default();
        assert_eq!(config.enabled, false);
        assert_eq!(config.view_type, StreamViewType::NewAndOldImages);
        assert_eq!(config.buffer_size, 1000);
    }

    #[test]
    fn test_stream_config_enabled() {
        let config = StreamConfig::enabled()
            .with_view_type(StreamViewType::KeysOnly)
            .with_buffer_size(500);

        assert_eq!(config.enabled, true);
        assert_eq!(config.view_type, StreamViewType::KeysOnly);
        assert_eq!(config.buffer_size, 500);
    }

    #[test]
    fn test_stream_record_insert() {
        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let record = StreamRecord::insert(1, key.clone(), item.clone(), StreamViewType::NewImage);

        assert_eq!(record.sequence_number, 1);
        assert_eq!(record.event_type, StreamEventType::Insert);
        assert_eq!(record.key, key);
        assert!(record.old_image.is_none());
        assert!(record.new_image.is_some());
    }

    #[test]
    fn test_stream_record_modify() {
        let key = Key::new(b"user#123".to_vec());
        let mut old_item = HashMap::new();
        old_item.insert("name".to_string(), Value::string("Alice"));
        let mut new_item = HashMap::new();
        new_item.insert("name".to_string(), Value::string("Bob"));

        let record = StreamRecord::modify(
            2,
            key.clone(),
            old_item.clone(),
            new_item.clone(),
            StreamViewType::NewAndOldImages,
        );

        assert_eq!(record.sequence_number, 2);
        assert_eq!(record.event_type, StreamEventType::Modify);
        assert!(record.old_image.is_some());
        assert!(record.new_image.is_some());
    }

    #[test]
    fn test_stream_record_remove() {
        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let record = StreamRecord::remove(3, key.clone(), item.clone(), StreamViewType::OldImage);

        assert_eq!(record.sequence_number, 3);
        assert_eq!(record.event_type, StreamEventType::Remove);
        assert!(record.old_image.is_some());
        assert!(record.new_image.is_none());
    }

    #[test]
    fn test_stream_record_keys_only() {
        let key = Key::new(b"user#123".to_vec());
        let mut item = HashMap::new();
        item.insert("name".to_string(), Value::string("Alice"));

        let record = StreamRecord::insert(1, key.clone(), item.clone(), StreamViewType::KeysOnly);

        assert!(record.old_image.is_none());
        assert!(record.new_image.is_none());
    }
}
