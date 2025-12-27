/// Extent allocation for SST heap
///
/// The SST heap region is managed as a collection of variable-size extents.
/// Each extent represents a contiguous range of blocks for an SST file.

use parking_lot::Mutex;
use std::collections::BTreeMap;
use crate::{Error, Result, layout::BLOCK_SIZE};

/// Maximum extent size (1GB) to prevent excessive allocations
const MAX_EXTENT_SIZE: u64 = 1024 * 1024 * 1024;

/// Extent ID - unique identifier for an extent
pub type ExtentId = u64;

/// An allocated extent in the SST heap
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Extent {
    pub id: ExtentId,
    /// Byte offset from start of file
    pub offset: u64,
    /// Size in bytes
    pub size: u64,
}

impl Extent {
    pub fn new(id: ExtentId, offset: u64, size: u64) -> Self {
        Self { id, offset, size }
    }

    pub fn end(&self) -> Result<u64> {
        self.offset.checked_add(self.size)
            .ok_or_else(|| Error::Internal("Extent end offset overflow".into()))
    }

    /// Number of blocks in this extent
    pub fn num_blocks(&self) -> Result<u64> {
        let block_size = BLOCK_SIZE as u64;
        let numerator = self.size.checked_add(block_size.saturating_sub(1))
            .ok_or_else(|| Error::Internal("Block count calculation overflow".into()))?;
        Ok(numerator / block_size)
    }
}

/// Extent allocator for SST heap
///
/// Uses a simple bump allocator strategy:
/// - Allocations always extend the file
/// - Free space tracking is deferred to compaction
pub struct ExtentAllocator {
    inner: Mutex<ExtentAllocatorInner>,
}

struct ExtentAllocatorInner {
    /// Base offset of SST heap in file
    base_offset: u64,
    /// Next extent ID
    next_id: ExtentId,
    /// Current end of allocated space (relative to base_offset)
    allocated_end: u64,
    /// Map of extent_id -> extent
    extents: BTreeMap<ExtentId, Extent>,
}

impl ExtentAllocator {
    /// Create a new allocator
    pub fn new(base_offset: u64) -> Self {
        Self {
            inner: Mutex::new(ExtentAllocatorInner {
                base_offset,
                next_id: 1,
                allocated_end: 0,
                extents: BTreeMap::new(),
            }),
        }
    }

    /// Allocate a new extent with the given size (in bytes)
    pub fn allocate(&self, size: u64) -> Result<Extent> {
        if size == 0 {
            return Err(Error::InvalidArgument("Extent size must be > 0".to_string()));
        }

        if size > MAX_EXTENT_SIZE {
            return Err(Error::InvalidArgument(
                format!("Extent size {} exceeds maximum {}", size, MAX_EXTENT_SIZE)
            ));
        }

        let mut inner = self.inner.lock();

        let id = inner.next_id;
        inner.next_id = inner.next_id.checked_add(1)
            .ok_or_else(|| Error::Internal("Extent ID overflow".into()))?;

        // Align size to block boundary
        let aligned_size = align_to_block(size)?;

        let offset = inner.base_offset.checked_add(inner.allocated_end)
            .ok_or_else(|| Error::Internal("Extent offset overflow".into()))?;

        let extent = Extent::new(id, offset, aligned_size);

        inner.allocated_end = inner.allocated_end.checked_add(aligned_size)
            .ok_or_else(|| Error::Internal("Extent allocator overflow".into()))?;

        inner.extents.insert(id, extent);

        Ok(extent)
    }

    /// Free an extent (mark for future compaction)
    pub fn free(&self, id: ExtentId) -> Result<()> {
        let mut inner = self.inner.lock();

        if inner.extents.remove(&id).is_none() {
            return Err(Error::InvalidArgument(format!("Extent {} not found", id)));
        }

        // Note: We don't reclaim space immediately. This will be done during compaction.
        Ok(())
    }

    /// Get an extent by ID
    pub fn get(&self, id: ExtentId) -> Option<Extent> {
        let inner = self.inner.lock();
        inner.extents.get(&id).copied()
    }

    /// Get total allocated size (in bytes)
    pub fn allocated_size(&self) -> u64 {
        let inner = self.inner.lock();
        inner.allocated_end
    }

    /// Get number of allocated extents
    pub fn num_extents(&self) -> usize {
        let inner = self.inner.lock();
        inner.extents.len()
    }

    /// List all allocated extents
    pub fn list_extents(&self) -> Vec<Extent> {
        let inner = self.inner.lock();
        inner.extents.values().copied().collect()
    }
}

/// Align size to block boundary (round up)
fn align_to_block(size: u64) -> Result<u64> {
    let block_size = BLOCK_SIZE as u64;
    let numerator = size.checked_add(block_size.saturating_sub(1))
        .ok_or_else(|| Error::Internal("Block alignment overflow".into()))?;
    let blocks = numerator / block_size;
    blocks.checked_mul(block_size)
        .ok_or_else(|| Error::Internal("Aligned size overflow".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_align_to_block() {
        assert_eq!(align_to_block(0).unwrap(), 0);
        assert_eq!(align_to_block(1).unwrap(), BLOCK_SIZE as u64);
        assert_eq!(align_to_block(4096).unwrap(), BLOCK_SIZE as u64);
        assert_eq!(align_to_block(4097).unwrap(), 2 * BLOCK_SIZE as u64);
        assert_eq!(align_to_block(8192).unwrap(), 2 * BLOCK_SIZE as u64);
    }

    #[test]
    fn test_align_to_block_overflow() {
        // Test overflow protection
        let result = align_to_block(u64::MAX);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_extent_allocator_basic() {
        let base = 1024 * 1024; // 1MB base offset
        let allocator = ExtentAllocator::new(base);

        // Allocate first extent
        let ext1 = allocator.allocate(1000).unwrap();
        assert_eq!(ext1.id, 1);
        assert_eq!(ext1.offset, base);
        assert_eq!(ext1.size, BLOCK_SIZE as u64); // Aligned

        // Allocate second extent
        let ext2 = allocator.allocate(5000).unwrap();
        assert_eq!(ext2.id, 2);
        assert_eq!(ext2.offset, base + BLOCK_SIZE as u64);
        assert_eq!(ext2.size, 2 * BLOCK_SIZE as u64); // 5000 rounds to 8192

        assert_eq!(allocator.num_extents(), 2);
    }

    #[test]
    fn test_extent_allocator_free() {
        let allocator = ExtentAllocator::new(0);

        let ext1 = allocator.allocate(1000).unwrap();
        let ext2 = allocator.allocate(2000).unwrap();

        assert_eq!(allocator.num_extents(), 2);

        allocator.free(ext1.id).unwrap();
        assert_eq!(allocator.num_extents(), 1);

        // Can't free twice
        let result = allocator.free(ext1.id);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));

        // Can get remaining extent
        let retrieved = allocator.get(ext2.id).unwrap();
        assert_eq!(retrieved, ext2);
    }

    #[test]
    fn test_extent_allocator_allocated_size() {
        let allocator = ExtentAllocator::new(1000);

        assert_eq!(allocator.allocated_size(), 0);

        allocator.allocate(4096).unwrap();
        assert_eq!(allocator.allocated_size(), 4096);

        allocator.allocate(100).unwrap(); // Aligns to 4096
        assert_eq!(allocator.allocated_size(), 8192);
    }

    #[test]
    fn test_extent_num_blocks() {
        let ext1 = Extent::new(1, 0, 4096);
        assert_eq!(ext1.num_blocks().unwrap(), 1);

        let ext2 = Extent::new(2, 0, 8192);
        assert_eq!(ext2.num_blocks().unwrap(), 2);

        let ext3 = Extent::new(3, 0, 5000);
        assert_eq!(ext3.num_blocks().unwrap(), 2); // 5000 bytes = 2 blocks
    }

    #[test]
    fn test_extent_overflow_protection() {
        let allocator = ExtentAllocator::new(0);

        // Test maximum extent size limit
        let result = allocator.allocate(MAX_EXTENT_SIZE + 1);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));

        // Test allocated_end overflow
        // Manually set allocated_end to near max, then try to allocate
        let allocator_overflow = ExtentAllocator::new(0);
        {
            let mut inner = allocator_overflow.inner.lock();
            inner.allocated_end = u64::MAX - 1000;
        }
        let result = allocator_overflow.allocate(8000); // Will overflow allocated_end
        assert!(matches!(result, Err(Error::Internal(_))));

        // Test base_offset + allocated_end overflow (after multiple allocations)
        let allocator_base = ExtentAllocator::new(u64::MAX - 5000);
        allocator_base.allocate(5000).unwrap(); // First allocation: 8192 bytes aligned
        let result = allocator_base.allocate(1000); // Second allocation: base + 8192 overflows
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_extent_id_overflow() {
        let allocator = ExtentAllocator::new(0);

        // Set next_id to near max
        {
            let mut inner = allocator.inner.lock();
            inner.next_id = u64::MAX;
        }

        // This should fail due to ID overflow
        let result = allocator.allocate(100);
        assert!(matches!(result, Err(Error::Internal(_))));
    }

    #[test]
    fn test_extent_list() {
        let allocator = ExtentAllocator::new(0);

        let ext1 = allocator.allocate(1000).unwrap();
        let ext2 = allocator.allocate(2000).unwrap();
        let ext3 = allocator.allocate(3000).unwrap();

        let extents = allocator.list_extents();
        assert_eq!(extents.len(), 3);
        assert!(extents.contains(&ext1));
        assert!(extents.contains(&ext2));
        assert!(extents.contains(&ext3));
    }

    #[test]
    fn test_extent_zero_size() {
        let allocator = ExtentAllocator::new(0);
        let result = allocator.allocate(0);
        assert!(matches!(result, Err(Error::InvalidArgument(_))));
    }
}
