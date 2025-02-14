use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::{self, Duration};

/// Scheduler for managing async callbacks with timeouts
#[derive(Debug)]
pub struct Scheduler {
    /// Map of scheduled tasks identified by their keys
    timeouts: Arc<Mutex<HashMap<String, JoinHandle<()>>>>,
}

impl Scheduler {
    /// Creates a new Scheduler instance
    pub fn new() -> Self {
        Self {
            timeouts: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Schedule a callback to be executed after the specified timeout
    ///
    /// # Arguments
    /// * `callback` - The callback to execute
    /// * `timeout` - The timeout in milliseconds
    /// * `key` - Unique identifier for this scheduled task
    pub async fn schedule<F>(&self, callback: F, timeout_ms: u64, key: String)
    where
        F: FnOnce() + Send + 'static,
    {
        // Cancel any existing task with the same key
        self.cancel(&key).await;

        let timeouts = self.timeouts.clone();

        // Create new task
        let handle = tokio::spawn(async move {
            time::sleep(Duration::from_millis(timeout_ms)).await;

            // Execute callback
            callback();

            // Remove self from timeouts map
            let mut timeouts = timeouts.lock().await;
            timeouts.remove(&key);
        });

        // Store the task handle
        let mut timeouts = self.timeouts.lock().await;
        timeouts.insert(key, handle);
    }

    /// Cancel a scheduled task by its key
    ///
    /// # Arguments
    /// * `key` - The key of the task to cancel
    pub async fn cancel(&self, key: &str) {
        let mut timeouts = self.timeouts.lock().await;
        if let Some(handle) = timeouts.remove(key) {
            handle.abort();
        }
    }

    /// Cancel all scheduled tasks
    pub async fn clear(&self) {
        let mut timeouts = self.timeouts.lock().await;
        for (_, handle) in timeouts.drain() {
            handle.abort();
        }
    }

    /// Check if a task with the given key exists
    ///
    /// # Arguments
    /// * `key` - The key to check
    pub async fn has(&self, key: &str) -> bool {
        let timeouts = self.timeouts.lock().await;
        timeouts.contains_key(key)
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[tokio::test]
    async fn test_schedule_and_execute() {
        let scheduler = Scheduler::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        scheduler.schedule(
            move || {
                called_clone.store(true, Ordering::SeqCst);
            },
            100,
            "test".to_string()
        ).await;

        assert!(scheduler.has("test").await);
        time::sleep(Duration::from_millis(150)).await;
        assert!(!scheduler.has("test").await);
        assert!(called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_cancel() {
        let scheduler = Scheduler::new();
        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        scheduler.schedule(
            move || {
                called_clone.store(true, Ordering::SeqCst);
            },
            100,
            "test".to_string()
        ).await;

        assert!(scheduler.has("test").await);
        scheduler.cancel("test").await;
        assert!(!scheduler.has("test").await);

        time::sleep(Duration::from_millis(150)).await;
        assert!(!called.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_clear() {
        let scheduler = Scheduler::new();

        for i in 0..3 {
            scheduler.schedule(
                move || {},
                100,
                format!("test{}", i)
            ).await;
        }

        assert!(scheduler.has("test0").await);
        assert!(scheduler.has("test1").await);
        assert!(scheduler.has("test2").await);

        scheduler.clear().await;

        assert!(!scheduler.has("test0").await);
        assert!(!scheduler.has("test1").await);
        assert!(!scheduler.has("test2").await);
    }
}
