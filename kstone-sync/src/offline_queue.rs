/// Offline queue for pending sync operations
///
/// Manages operations that couldn't be synced due to network issues
/// and retries them when connectivity is restored.

use anyhow::Result;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use kstone_core::{Item, Key};
use crate::{EndpointId, VectorClock};

/// Type of pending operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationType {
    /// Put/Update operation
    Put,
    /// Delete operation
    Delete,
    /// Batch write operation
    BatchWrite,
}

/// A pending operation in the offline queue
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOperation {
    /// Unique ID for this operation
    pub id: String,
    /// Type of operation
    pub operation_type: OperationType,
    /// Target endpoint
    pub endpoint_id: EndpointId,
    /// Key(s) affected
    pub keys: Vec<Key>,
    /// Item data (for put operations)
    pub items: Vec<Option<Item>>,
    /// Vector clock at time of operation
    pub vector_clock: VectorClock,
    /// When this operation was created
    pub created_at: i64,
    /// Last retry attempt
    pub last_retry_at: Option<i64>,
    /// Number of retry attempts
    pub retry_count: u32,
    /// Error from last attempt (if any)
    pub last_error: Option<String>,
    /// Priority (higher = more important)
    pub priority: i32,
}

impl PendingOperation {
    /// Create a new put operation
    pub fn put(
        endpoint_id: EndpointId,
        key: Key,
        item: Item,
        vector_clock: VectorClock,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            operation_type: OperationType::Put,
            endpoint_id,
            keys: vec![key],
            items: vec![Some(item)],
            vector_clock,
            created_at: chrono::Utc::now().timestamp_millis(),
            last_retry_at: None,
            retry_count: 0,
            last_error: None,
            priority: 0,
        }
    }

    /// Create a new delete operation
    pub fn delete(
        endpoint_id: EndpointId,
        key: Key,
        vector_clock: VectorClock,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            operation_type: OperationType::Delete,
            endpoint_id,
            keys: vec![key],
            items: vec![None],
            vector_clock,
            created_at: chrono::Utc::now().timestamp_millis(),
            last_retry_at: None,
            retry_count: 0,
            last_error: None,
            priority: 0,
        }
    }

    /// Create a batch write operation
    pub fn batch_write(
        endpoint_id: EndpointId,
        keys: Vec<Key>,
        items: Vec<Option<Item>>,
        vector_clock: VectorClock,
    ) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            operation_type: OperationType::BatchWrite,
            endpoint_id,
            keys,
            items,
            vector_clock,
            created_at: chrono::Utc::now().timestamp_millis(),
            last_retry_at: None,
            retry_count: 0,
            last_error: None,
            priority: 0,
        }
    }

    /// Mark a retry attempt
    pub fn mark_retry(&mut self, error: Option<String>) {
        self.retry_count += 1;
        self.last_retry_at = Some(chrono::Utc::now().timestamp_millis());
        self.last_error = error;
    }

    /// Check if we should retry this operation
    pub fn should_retry(&self, max_retries: u32, backoff_ms: u64) -> bool {
        if self.retry_count >= max_retries {
            return false;
        }

        if let Some(last_retry) = self.last_retry_at {
            let backoff_duration = backoff_ms * 2_u64.pow(self.retry_count.min(10));
            let next_retry_time = last_retry + backoff_duration as i64;
            let now = chrono::Utc::now().timestamp_millis();
            now >= next_retry_time
        } else {
            true
        }
    }

    /// Get age of this operation in milliseconds
    pub fn age_ms(&self) -> i64 {
        chrono::Utc::now().timestamp_millis() - self.created_at
    }

    /// Check if operation is expired
    pub fn is_expired(&self, max_age_ms: i64) -> bool {
        self.age_ms() > max_age_ms
    }
}

/// Retry policy for offline operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff in milliseconds
    pub initial_backoff_ms: u64,
    /// Maximum backoff in milliseconds
    pub max_backoff_ms: u64,
    /// Whether to use exponential backoff
    pub exponential_backoff: bool,
    /// Maximum age for operations (ms)
    pub max_operation_age_ms: i64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_backoff_ms: 1000,      // 1 second
            max_backoff_ms: 60_000,         // 1 minute
            exponential_backoff: true,
            max_operation_age_ms: 86_400_000, // 24 hours
        }
    }
}

/// Manages offline operations
pub struct OfflineQueue {
    /// Pending operations
    queue: Arc<RwLock<VecDeque<PendingOperation>>>,
    /// Operations being processed
    processing: Arc<RwLock<Vec<PendingOperation>>>,
    /// Failed operations (exceeded retry limit)
    failed: Arc<RwLock<Vec<PendingOperation>>>,
    /// Retry policy
    retry_policy: RetryPolicy,
    /// Maximum queue size
    max_queue_size: usize,
    /// Whether queue is paused
    paused: Arc<RwLock<bool>>,
}

impl OfflineQueue {
    /// Create a new offline queue
    pub fn new(retry_policy: RetryPolicy, max_queue_size: usize) -> Self {
        Self {
            queue: Arc::new(RwLock::new(VecDeque::new())),
            processing: Arc::new(RwLock::new(Vec::new())),
            failed: Arc::new(RwLock::new(Vec::new())),
            retry_policy,
            max_queue_size,
            paused: Arc::new(RwLock::new(false)),
        }
    }

    /// Enqueue an operation
    pub fn enqueue(&self, mut operation: PendingOperation) -> Result<()> {
        let mut queue = self.queue.write();

        // Check queue size limit
        if queue.len() >= self.max_queue_size {
            // Remove oldest low-priority operations
            queue.retain(|op| op.priority > 0 || op.age_ms() < 60_000);

            if queue.len() >= self.max_queue_size {
                return Err(anyhow::anyhow!("Offline queue is full"));
            }
        }

        // Insert based on priority
        let position = queue.iter().position(|op| op.priority < operation.priority)
            .unwrap_or(queue.len());

        queue.insert(position, operation);
        Ok(())
    }

    /// Get next operations to process
    pub fn get_next_batch(&self, batch_size: usize) -> Vec<PendingOperation> {
        if *self.paused.read() {
            return Vec::new();
        }

        let mut queue = self.queue.write();
        let mut processing = self.processing.write();
        let mut batch = Vec::new();

        // Clean up expired operations
        let max_age = self.retry_policy.max_operation_age_ms;
        queue.retain(|op| !op.is_expired(max_age));

        // Get operations that are ready for retry
        let now = chrono::Utc::now().timestamp_millis();

        while batch.len() < batch_size && !queue.is_empty() {
            if let Some(op) = queue.front() {
                if op.should_retry(
                    self.retry_policy.max_retries,
                    self.retry_policy.initial_backoff_ms,
                ) {
                    let mut op = queue.pop_front().unwrap();
                    processing.push(op.clone());
                    batch.push(op);
                } else {
                    break;
                }
            }
        }

        batch
    }

    /// Mark operation as completed
    pub fn mark_completed(&self, operation_id: &str) -> Result<()> {
        let mut processing = self.processing.write();
        processing.retain(|op| op.id != operation_id);
        Ok(())
    }

    /// Mark operation as failed and requeue if applicable
    pub fn mark_failed(&self, operation_id: &str, error: String) -> Result<()> {
        let mut processing = self.processing.write();
        let mut queue = self.queue.write();
        let mut failed = self.failed.write();

        if let Some(position) = processing.iter().position(|op| op.id == operation_id) {
            let mut op = processing.remove(position);
            op.mark_retry(Some(error));

            if op.retry_count >= self.retry_policy.max_retries {
                // Move to failed queue
                failed.push(op);
            } else {
                // Requeue with backoff
                queue.push_back(op);
            }
        }

        Ok(())
    }

    /// Get all operations for an endpoint
    pub fn get_endpoint_operations(&self, endpoint_id: &EndpointId) -> Vec<PendingOperation> {
        let queue = self.queue.read();
        let processing = self.processing.read();

        let mut operations: Vec<_> = queue
            .iter()
            .filter(|op| &op.endpoint_id == endpoint_id)
            .cloned()
            .collect();

        operations.extend(
            processing
                .iter()
                .filter(|op| &op.endpoint_id == endpoint_id)
                .cloned()
        );

        operations
    }

    /// Clear all operations for an endpoint
    pub fn clear_endpoint(&self, endpoint_id: &EndpointId) {
        self.queue.write().retain(|op| &op.endpoint_id != endpoint_id);
        self.processing.write().retain(|op| &op.endpoint_id != endpoint_id);
        self.failed.write().retain(|op| &op.endpoint_id != endpoint_id);
    }

    /// Pause the queue
    pub fn pause(&self) {
        *self.paused.write() = true;
    }

    /// Resume the queue
    pub fn resume(&self) {
        *self.paused.write() = false;
    }

    /// Check if queue is paused
    pub fn is_paused(&self) -> bool {
        *self.paused.read()
    }

    /// Get queue statistics
    pub fn get_stats(&self) -> QueueStats {
        QueueStats {
            pending: self.queue.read().len(),
            processing: self.processing.read().len(),
            failed: self.failed.read().len(),
            paused: self.is_paused(),
        }
    }

    /// Clear all queues
    pub fn clear(&self) {
        self.queue.write().clear();
        self.processing.write().clear();
        self.failed.write().clear();
    }

    /// Get failed operations
    pub fn get_failed(&self) -> Vec<PendingOperation> {
        self.failed.read().clone()
    }

    /// Retry failed operations
    pub fn retry_failed(&self) {
        let mut failed = self.failed.write();
        let mut queue = self.queue.write();

        for mut op in failed.drain(..) {
            op.retry_count = 0;
            op.last_retry_at = None;
            op.last_error = None;
            queue.push_back(op);
        }
    }

    /// Persist queue to storage
    pub fn persist(&self, path: &std::path::Path) -> Result<()> {
        let state = QueueState {
            queue: self.queue.read().clone().into_iter().collect(),
            processing: self.processing.read().clone(),
            failed: self.failed.read().clone(),
            retry_policy: self.retry_policy.clone(),
        };

        let json = serde_json::to_string(&state)?;
        std::fs::write(path, json)?;
        Ok(())
    }

    /// Restore queue from storage
    pub fn restore(&self, path: &std::path::Path) -> Result<()> {
        if !path.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(path)?;
        let state: QueueState = serde_json::from_str(&json)?;

        *self.queue.write() = state.queue.into_iter().collect();
        *self.processing.write() = state.processing;
        *self.failed.write() = state.failed;

        Ok(())
    }
}

/// Queue statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueStats {
    pub pending: usize,
    pub processing: usize,
    pub failed: usize,
    pub paused: bool,
}

/// Persisted queue state
#[derive(Debug, Clone, Serialize, Deserialize)]
struct QueueState {
    queue: Vec<PendingOperation>,
    processing: Vec<PendingOperation>,
    failed: Vec<PendingOperation>,
    retry_policy: RetryPolicy,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_offline_queue_basic() {
        let queue = OfflineQueue::new(RetryPolicy::default(), 100);

        let op = PendingOperation::put(
            EndpointId::from_str("remote"),
            Key::new(b"key1".to_vec()),
            HashMap::new(),
            VectorClock::new(),
        );

        queue.enqueue(op.clone()).unwrap();

        let batch = queue.get_next_batch(10);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].id, op.id);

        queue.mark_completed(&op.id).unwrap();

        let batch = queue.get_next_batch(10);
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn test_retry_backoff() {
        let mut policy = RetryPolicy::default();
        policy.max_retries = 3;
        policy.initial_backoff_ms = 100;

        let queue = OfflineQueue::new(policy, 100);

        let op = PendingOperation::delete(
            EndpointId::from_str("remote"),
            Key::new(b"key1".to_vec()),
            VectorClock::new(),
        );

        queue.enqueue(op.clone()).unwrap();

        // First attempt
        let batch = queue.get_next_batch(1);
        assert_eq!(batch.len(), 1);

        // Mark as failed
        queue.mark_failed(&op.id, "Network error".to_string()).unwrap();

        // Should not be available immediately
        let batch = queue.get_next_batch(1);
        assert_eq!(batch.len(), 0);

        // After backoff, should be available
        std::thread::sleep(Duration::from_millis(150));
        let batch = queue.get_next_batch(1);
        assert_eq!(batch.len(), 1);
    }

    #[test]
    fn test_priority_ordering() {
        let queue = OfflineQueue::new(RetryPolicy::default(), 100);

        let mut low_priority = PendingOperation::put(
            EndpointId::from_str("remote"),
            Key::new(b"key1".to_vec()),
            HashMap::new(),
            VectorClock::new(),
        );
        low_priority.priority = 0;

        let mut high_priority = PendingOperation::put(
            EndpointId::from_str("remote"),
            Key::new(b"key2".to_vec()),
            HashMap::new(),
            VectorClock::new(),
        );
        high_priority.priority = 10;

        queue.enqueue(low_priority.clone()).unwrap();
        queue.enqueue(high_priority.clone()).unwrap();

        let batch = queue.get_next_batch(1);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].id, high_priority.id);
    }
}