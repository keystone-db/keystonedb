/// Background task management for LSM operations
///
/// Provides a background worker thread that performs compaction operations
/// asynchronously without blocking database operations.

use crate::compaction::{CompactionConfig, CompactionStatsAtomic};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, info, warn};

/// Request to perform compaction on a stripe
#[derive(Debug, Clone)]
pub struct CompactionRequest {
    /// Stripe ID to compact
    pub stripe_id: usize,
}

/// Background worker for running compaction tasks
pub struct BackgroundWorker {
    /// Worker thread handle
    handle: Option<JoinHandle<()>>,

    /// Shutdown signal
    shutdown: Arc<AtomicBool>,

    /// Work queue (stripes needing compaction)
    work_queue: Arc<Mutex<Vec<CompactionRequest>>>,

    /// Compaction configuration
    config: CompactionConfig,

    /// Statistics
    stats: CompactionStatsAtomic,
}

impl BackgroundWorker {
    /// Create a new background worker
    pub fn new(config: CompactionConfig, stats: CompactionStatsAtomic) -> Self {
        Self {
            handle: None,
            shutdown: Arc::new(AtomicBool::new(false)),
            work_queue: Arc::new(Mutex::new(Vec::new())),
            config,
            stats,
        }
    }

    /// Start the background worker thread
    ///
    /// The worker will periodically check the work queue and process compaction requests.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            warn!("Background worker already running");
            return;
        }

        let shutdown = Arc::clone(&self.shutdown);
        let work_queue = Arc::clone(&self.work_queue);
        let config = self.config.clone();
        let stats = self.stats.clone();

        info!("Starting background compaction worker");

        let handle = thread::spawn(move || {
            Self::worker_loop(shutdown, work_queue, config, stats);
        });

        self.handle = Some(handle);
    }

    /// Worker loop that runs in background thread
    fn worker_loop(
        shutdown: Arc<AtomicBool>,
        work_queue: Arc<Mutex<Vec<CompactionRequest>>>,
        config: CompactionConfig,
        _stats: CompactionStatsAtomic,
    ) {
        debug!("Background worker loop started");

        let check_interval = Duration::from_secs(config.check_interval_secs);

        while !shutdown.load(Ordering::Relaxed) {
            // Check for work
            let work = {
                let mut queue = work_queue.lock().unwrap();
                if queue.is_empty() {
                    None
                } else {
                    // Take up to max_concurrent_compactions items
                    let count = queue.len().min(config.max_concurrent_compactions);
                    Some(queue.drain(..count).collect::<Vec<_>>())
                }
            };

            if let Some(requests) = work {
                debug!("Processing {} compaction requests", requests.len());

                for request in requests {
                    if shutdown.load(Ordering::Relaxed) {
                        debug!("Shutdown signal received, stopping compaction");
                        break;
                    }

                    // TODO: Actually perform compaction here
                    // This will be connected to LsmEngine in Task 4
                    debug!("Would compact stripe {}", request.stripe_id);
                }
            }

            // Sleep before checking again
            thread::sleep(check_interval);
        }

        info!("Background worker loop exited");
    }

    /// Queue a compaction request for a stripe
    pub fn queue_compaction(&self, stripe_id: usize) {
        let mut queue = self.work_queue.lock().unwrap();

        // Don't queue duplicate requests
        if queue.iter().any(|r| r.stripe_id == stripe_id) {
            debug!("Stripe {} already queued for compaction", stripe_id);
            return;
        }

        debug!("Queuing compaction for stripe {}", stripe_id);
        queue.push(CompactionRequest { stripe_id });
    }

    /// Get the current work queue size
    pub fn queue_size(&self) -> usize {
        self.work_queue.lock().unwrap().len()
    }

    /// Initiate graceful shutdown
    ///
    /// Signals the worker thread to stop and waits for it to finish.
    pub fn shutdown(&mut self) {
        info!("Initiating background worker shutdown");
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(handle) = self.handle.take() {
            debug!("Waiting for background worker thread to exit");
            if let Err(e) = handle.join() {
                warn!("Error joining background worker thread: {:?}", e);
            }
        }

        info!("Background worker shutdown complete");
    }

    /// Check if the worker is running
    pub fn is_running(&self) -> bool {
        self.handle.is_some() && !self.shutdown.load(Ordering::Relaxed)
    }
}

impl Drop for BackgroundWorker {
    fn drop(&mut self) {
        if self.is_running() {
            self.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_worker_start_stop() {
        let config = CompactionConfig::new();
        let stats = CompactionStatsAtomic::new();
        let mut worker = BackgroundWorker::new(config, stats);

        assert!(!worker.is_running());

        worker.start();
        assert!(worker.is_running());

        worker.shutdown();
        assert!(!worker.is_running());
    }

    #[test]
    fn test_background_worker_queue() {
        let config = CompactionConfig::new();
        let stats = CompactionStatsAtomic::new();
        let worker = BackgroundWorker::new(config, stats);

        assert_eq!(worker.queue_size(), 0);

        worker.queue_compaction(1);
        assert_eq!(worker.queue_size(), 1);

        worker.queue_compaction(2);
        assert_eq!(worker.queue_size(), 2);

        // Duplicate should not be queued
        worker.queue_compaction(1);
        assert_eq!(worker.queue_size(), 2);
    }

    #[test]
    fn test_background_worker_auto_shutdown_on_drop() {
        let config = CompactionConfig::new();
        let stats = CompactionStatsAtomic::new();
        let mut worker = BackgroundWorker::new(config, stats);

        worker.start();
        assert!(worker.is_running());

        // Drop should trigger shutdown
        drop(worker);
        // If this test completes, shutdown worked
    }

    #[test]
    fn test_background_worker_disabled_config() {
        let config = CompactionConfig::disabled();
        let stats = CompactionStatsAtomic::new();
        let mut worker = BackgroundWorker::new(config.clone(), stats);

        // Start worker even though config is disabled
        // (The actual compaction logic will check config.enabled)
        worker.start();
        assert!(worker.is_running());

        worker.shutdown();
    }

    #[test]
    fn test_background_worker_multiple_queued_items() {
        let config = CompactionConfig::new().with_max_concurrent(2);
        let stats = CompactionStatsAtomic::new();
        let worker = BackgroundWorker::new(config, stats);

        // Queue multiple stripes
        for i in 0..10 {
            worker.queue_compaction(i);
        }

        assert_eq!(worker.queue_size(), 10);
    }
}
