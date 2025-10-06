/// Database configuration for resource limits and operational parameters
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    /// Maximum memtable size in bytes (None = unlimited)
    pub max_memtable_size_bytes: Option<usize>,

    /// Maximum number of records in memtable before flush
    pub max_memtable_records: usize,

    /// Maximum WAL size in bytes (None = unlimited)
    pub max_wal_size_bytes: Option<u64>,

    /// Maximum total disk space in bytes (None = unlimited)
    pub max_total_disk_bytes: Option<u64>,

    /// Write buffer size for WAL/SST writes
    pub write_buffer_size: usize,

    /// Enable compression for SST files
    pub compression_enabled: bool,

    /// Compression level (1-22, where 1 is fastest, 22 is best compression)
    /// Default: 3 (balanced speed/ratio)
    pub compression_level: i32,
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            max_memtable_size_bytes: None,
            max_memtable_records: 1000,
            max_wal_size_bytes: None,
            max_total_disk_bytes: None,
            write_buffer_size: 1024,
            compression_enabled: false,
            compression_level: 3,
        }
    }
}

impl DatabaseConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Set maximum memtable size in bytes
    pub fn with_max_memtable_size_bytes(mut self, size: usize) -> Self {
        self.max_memtable_size_bytes = Some(size);
        self
    }

    /// Set maximum number of records in memtable
    pub fn with_max_memtable_records(mut self, records: usize) -> Self {
        self.max_memtable_records = records;
        self
    }

    /// Set maximum WAL size in bytes
    pub fn with_max_wal_size_bytes(mut self, size: u64) -> Self {
        self.max_wal_size_bytes = Some(size);
        self
    }

    /// Set maximum total disk size in bytes
    pub fn with_max_total_disk_bytes(mut self, size: u64) -> Self {
        self.max_total_disk_bytes = Some(size);
        self
    }

    /// Set write buffer size
    pub fn with_write_buffer_size(mut self, size: usize) -> Self {
        self.write_buffer_size = size;
        self
    }

    /// Enable compression for SST files
    pub fn with_compression(mut self) -> Self {
        self.compression_enabled = true;
        self
    }

    /// Set compression level (1-22)
    /// Level 1 is fastest, level 22 provides best compression
    /// Default: 3 (balanced speed/ratio)
    pub fn with_compression_level(mut self, level: i32) -> Self {
        self.compression_enabled = true;
        self.compression_level = level.clamp(1, 22);
        self
    }

    /// Validate configuration values
    pub fn validate(&self) -> Result<(), String> {
        if self.max_memtable_records == 0 {
            return Err("max_memtable_records must be greater than 0".to_string());
        }

        if self.write_buffer_size == 0 {
            return Err("write_buffer_size must be greater than 0".to_string());
        }

        if let Some(size) = self.max_memtable_size_bytes {
            if size == 0 {
                return Err("max_memtable_size_bytes must be greater than 0 when set".to_string());
            }
        }

        if self.compression_level < 1 || self.compression_level > 22 {
            return Err("compression_level must be between 1 and 22".to_string());
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = DatabaseConfig::default();
        assert_eq!(config.max_memtable_records, 1000);
        assert_eq!(config.write_buffer_size, 1024);
        assert!(config.max_memtable_size_bytes.is_none());
        assert!(config.max_wal_size_bytes.is_none());
        assert!(config.max_total_disk_bytes.is_none());
    }

    #[test]
    fn test_builder_methods() {
        let config = DatabaseConfig::new()
            .with_max_memtable_size_bytes(1024 * 1024)
            .with_max_memtable_records(500)
            .with_max_wal_size_bytes(10 * 1024 * 1024)
            .with_write_buffer_size(2048);

        assert_eq!(config.max_memtable_size_bytes, Some(1024 * 1024));
        assert_eq!(config.max_memtable_records, 500);
        assert_eq!(config.max_wal_size_bytes, Some(10 * 1024 * 1024));
        assert_eq!(config.write_buffer_size, 2048);
    }

    #[test]
    fn test_validate_success() {
        let config = DatabaseConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_zero_records() {
        let config = DatabaseConfig::new().with_max_memtable_records(0);
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_validate_zero_buffer_size() {
        let config = DatabaseConfig::new().with_write_buffer_size(0);
        assert!(config.validate().is_err());
    }
}
