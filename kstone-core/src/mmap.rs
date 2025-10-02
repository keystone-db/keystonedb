/// Memory-mapped file reader pool
///
/// Provides efficient read-only access to file data using memory mapping.
/// Multiple readers can share the same mmap, with automatic lifecycle management.

use bytes::Bytes;
use memmap2::Mmap;
use parking_lot::RwLock;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::{Error, Result};

/// Memory-mapped reader
#[derive(Clone)]
pub struct MmapReader {
    inner: Arc<MmapReaderInner>,
}

struct MmapReaderInner {
    path: PathBuf,
    mmap: RwLock<Option<Mmap>>,
}

impl MmapReader {
    /// Open a file for memory-mapped reading
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let file = File::open(&path)?;

        // SAFETY: We only create read-only mmaps and ensure file isn't modified
        let mmap = unsafe { Mmap::map(&file)? };

        Ok(Self {
            inner: Arc::new(MmapReaderInner {
                path,
                mmap: RwLock::new(Some(mmap)),
            }),
        })
    }

    /// Read bytes from the given offset
    pub fn read(&self, offset: u64, len: usize) -> Result<Bytes> {
        let mmap_guard = self.inner.mmap.read();
        let mmap = mmap_guard.as_ref()
            .ok_or_else(|| Error::Internal("Mmap has been closed".to_string()))?;

        let offset = offset as usize;
        let end = offset.checked_add(len)
            .ok_or_else(|| Error::InvalidArgument("Read would overflow".to_string()))?;

        if end > mmap.len() {
            return Err(Error::InvalidArgument(format!(
                "Read beyond file bounds: {} + {} > {}",
                offset, len, mmap.len()
            )));
        }

        Ok(Bytes::copy_from_slice(&mmap[offset..end]))
    }

    /// Read entire block at offset
    pub fn read_block(&self, offset: u64) -> Result<Bytes> {
        self.read(offset, crate::layout::BLOCK_SIZE)
    }

    /// Get file size
    pub fn len(&self) -> usize {
        let mmap_guard = self.inner.mmap.read();
        mmap_guard.as_ref().map(|m| m.len()).unwrap_or(0)
    }

    /// Check if file is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Get file path
    pub fn path(&self) -> &Path {
        &self.inner.path
    }

    /// Close the mmap (releases resources)
    pub fn close(&self) {
        let mut mmap_guard = self.inner.mmap.write();
        *mmap_guard = None;
    }
}

/// Pool of memory-mapped readers
///
/// Manages a cache of MmapReaders to avoid repeatedly mapping the same files.
pub struct MmapPool {
    readers: RwLock<std::collections::HashMap<PathBuf, MmapReader>>,
}

impl MmapPool {
    /// Create a new mmap pool
    pub fn new() -> Self {
        Self {
            readers: RwLock::new(std::collections::HashMap::new()),
        }
    }

    /// Get or create a reader for the given path
    pub fn get_or_open(&self, path: impl AsRef<Path>) -> Result<MmapReader> {
        let path = path.as_ref();

        // Fast path: check if already in pool
        {
            let readers = self.readers.read();
            if let Some(reader) = readers.get(path) {
                return Ok(reader.clone());
            }
        }

        // Slow path: open new reader
        let reader = MmapReader::open(path)?;

        let mut readers = self.readers.write();
        readers.insert(path.to_path_buf(), reader.clone());

        Ok(reader)
    }

    /// Remove a reader from the pool
    pub fn remove(&self, path: impl AsRef<Path>) {
        let mut readers = self.readers.write();
        if let Some(reader) = readers.remove(path.as_ref()) {
            reader.close();
        }
    }

    /// Clear all readers from the pool
    pub fn clear(&self) {
        let mut readers = self.readers.write();
        for (_, reader) in readers.drain() {
            reader.close();
        }
    }

    /// Get number of cached readers
    pub fn len(&self) -> usize {
        let readers = self.readers.read();
        readers.len()
    }

    /// Check if pool is empty
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for MmapPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;
    use std::io::Write;

    #[test]
    fn test_mmap_reader_basic() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"hello world").unwrap();
        tmp.flush().unwrap();

        let reader = MmapReader::open(tmp.path()).unwrap();

        let data = reader.read(0, 5).unwrap();
        assert_eq!(&data[..], b"hello");

        let data = reader.read(6, 5).unwrap();
        assert_eq!(&data[..], b"world");

        assert_eq!(reader.len(), 11);
        assert!(!reader.is_empty());
    }

    #[test]
    fn test_mmap_reader_out_of_bounds() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"test").unwrap();
        tmp.flush().unwrap();

        let reader = MmapReader::open(tmp.path()).unwrap();

        // Read beyond end
        let result = reader.read(0, 100);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));

        // Offset beyond end
        let result = reader.read(100, 1);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));
    }

    #[test]
    fn test_mmap_reader_close() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"data").unwrap();
        tmp.flush().unwrap();

        let reader = MmapReader::open(tmp.path()).unwrap();

        let data = reader.read(0, 4).unwrap();
        assert_eq!(&data[..], b"data");

        reader.close();

        // Can't read after close
        let result = reader.read(0, 1);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_mmap_pool_basic() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"pool test").unwrap();
        tmp.flush().unwrap();

        let pool = MmapPool::new();
        assert!(pool.is_empty());

        // First get opens the file
        let reader1 = pool.get_or_open(tmp.path()).unwrap();
        assert_eq!(pool.len(), 1);

        // Second get returns cached reader
        let reader2 = pool.get_or_open(tmp.path()).unwrap();
        assert_eq!(pool.len(), 1);

        // Both readers should work
        let data1 = reader1.read(0, 4).unwrap();
        let data2 = reader2.read(0, 4).unwrap();
        assert_eq!(data1, data2);
    }

    #[test]
    fn test_mmap_pool_remove() {
        let mut tmp = NamedTempFile::new().unwrap();
        tmp.write_all(b"test").unwrap();
        tmp.flush().unwrap();

        let pool = MmapPool::new();
        let _reader = pool.get_or_open(tmp.path()).unwrap();

        assert_eq!(pool.len(), 1);
        pool.remove(tmp.path());
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_mmap_pool_clear() {
        let mut tmp1 = NamedTempFile::new().unwrap();
        tmp1.write_all(b"file1").unwrap();
        tmp1.flush().unwrap();

        let mut tmp2 = NamedTempFile::new().unwrap();
        tmp2.write_all(b"file2").unwrap();
        tmp2.flush().unwrap();

        let pool = MmapPool::new();
        pool.get_or_open(tmp1.path()).unwrap();
        pool.get_or_open(tmp2.path()).unwrap();

        assert_eq!(pool.len(), 2);
        pool.clear();
        assert_eq!(pool.len(), 0);
    }

    #[test]
    fn test_mmap_read_block() {
        let mut tmp = NamedTempFile::new().unwrap();
        let data = vec![0xAB; crate::layout::BLOCK_SIZE];
        tmp.write_all(&data).unwrap();
        tmp.flush().unwrap();

        let reader = MmapReader::open(tmp.path()).unwrap();
        let block = reader.read_block(0).unwrap();

        assert_eq!(block.len(), crate::layout::BLOCK_SIZE);
        assert_eq!(&block[..], &data[..]);
    }
}
