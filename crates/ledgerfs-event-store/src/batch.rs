//! Batch Operations for High-Throughput Event Processing
//! 
//! This module provides batching capabilities to achieve the target >50K TPS
//! by grouping multiple events into atomic write operations.

use crate::{
    store::{EventStore, EventStoreError},
    serialization::generate_event_key,
};
use ledgerfs_core::*;
use rocksdb::WriteBatch;
use std::collections::VecDeque;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Mutex, Notify};
use tokio::time::sleep;

/// Configuration for batch processing
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum number of events per batch
    pub max_batch_size: usize,
    
    /// Maximum time to wait before flushing a partial batch
    pub max_batch_delay: Duration,
    
    /// Size of the event buffer queue
    pub buffer_size: usize,
    
    /// Number of concurrent batch processors
    pub num_processors: usize,
    
    /// Enable optimized append operations for high throughput
    pub use_optimized_append: bool,
    
    /// Target throughput in events per second (for adaptive batching)
    pub target_throughput: u64,
    
    /// Enable append-only validation (prevents overwrites)
    pub enforce_append_only: bool,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 1000,
            max_batch_delay: Duration::from_millis(10),
            buffer_size: 10000,
            num_processors: 4,
            use_optimized_append: true,
            target_throughput: 50000, // Target >50K TPS
            enforce_append_only: true,
        }
    }
}

impl BatchConfig {
    /// Create a high-throughput configuration for >50K TPS
    pub fn high_throughput() -> Self {
        Self {
            max_batch_size: 2000,
            max_batch_delay: Duration::from_millis(5),
            buffer_size: 50000,
            num_processors: 8,
            use_optimized_append: true,
            target_throughput: 75000,
            enforce_append_only: true,
        }
    }
    
    /// Create a low-latency configuration
    pub fn low_latency() -> Self {
        Self {
            max_batch_size: 100,
            max_batch_delay: Duration::from_millis(1),
            buffer_size: 5000,
            num_processors: 2,
            use_optimized_append: true,
            target_throughput: 10000,
            enforce_append_only: true,
        }
    }
}

/// A batch of events ready for processing
#[derive(Debug)]
pub struct EventBatch {
    pub events: Vec<EventEnvelope>,
    pub created_at: Instant,
}

impl EventBatch {
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            created_at: Instant::now(),
        }
    }
    
    pub fn add_event(&mut self, event: EventEnvelope) {
        self.events.push(event);
    }
    
    pub fn is_full(&self, max_size: usize) -> bool {
        self.events.len() >= max_size
    }
    
    pub fn is_expired(&self, max_delay: Duration) -> bool {
        self.created_at.elapsed() >= max_delay
    }
    
    pub fn len(&self) -> usize {
        self.events.len()
    }
    
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

/// High-throughput batch processor for events
pub struct BatchProcessor {
    event_store: Arc<EventStore>,
    config: BatchConfig,
    event_sender: mpsc::Sender<EventEnvelope>,
    shutdown_notify: Arc<Notify>,
    metrics: Arc<Mutex<BatchMetrics>>,
}

/// Metrics for batch processing performance
#[derive(Debug, Default, Clone)]
pub struct BatchMetrics {
    pub total_events_processed: u64,
    pub total_batches_processed: u64,
    pub average_batch_size: f64,
    pub events_per_second: f64,
    pub last_batch_time: Option<Instant>,
    pub processing_errors: u64,
    pub total_bytes_written: u64,
    pub average_write_latency: Duration,
    pub peak_throughput: f64,
    pub append_only_violations: u64,
}

impl BatchProcessor {
    /// Create a new batch processor
    pub fn new(event_store: Arc<EventStore>, config: BatchConfig) -> Self {
        let (event_sender, _) = mpsc::channel(config.buffer_size);
        
        Self {
            event_store,
            config,
            event_sender,
            shutdown_notify: Arc::new(Notify::new()),
            metrics: Arc::new(Mutex::new(BatchMetrics::default())),
        }
    }
    
    /// Start the batch processing system
    pub async fn start(&self) -> Result<mpsc::Sender<EventEnvelope>, EventStoreError> {
        let (event_sender, event_receiver) = mpsc::channel(self.config.buffer_size);
        let event_receiver = Arc::new(Mutex::new(event_receiver));
        
        // Start batch processors
        for i in 0..self.config.num_processors {
            let processor_id = i;
            let event_store = Arc::clone(&self.event_store);
            let config = self.config.clone();
            let shutdown_notify = Arc::clone(&self.shutdown_notify);
            let metrics = Arc::clone(&self.metrics);
            let receiver = Arc::clone(&event_receiver);
            
            tokio::spawn(async move {
                Self::run_batch_processor(
                    processor_id,
                    event_store,
                    config,
                    receiver,
                    shutdown_notify,
                    metrics,
                ).await;
            });
        }
        
        Ok(event_sender)
    }
    
    /// Run a single batch processor
    async fn run_batch_processor(
        processor_id: usize,
        event_store: Arc<EventStore>,
        config: BatchConfig,
        event_receiver: Arc<Mutex<mpsc::Receiver<EventEnvelope>>>,
        shutdown_notify: Arc<Notify>,
        metrics: Arc<Mutex<BatchMetrics>>,
    ) {
        let mut current_batch = EventBatch::new();
        let mut batch_timer = tokio::time::interval(config.max_batch_delay);
        
        loop {
            tokio::select! {
                // Receive new events
                event_result = async {
                    let mut receiver = event_receiver.lock().await;
                    receiver.recv().await
                } => {
                    match event_result {
                        Some(envelope) => {
                            current_batch.add_event(envelope);
                            
                            // Flush if batch is full
                            if current_batch.is_full(config.max_batch_size) {
                                Self::flush_batch(
                                    processor_id,
                                    &event_store,
                                    &mut current_batch,
                                    &metrics,
                                    &config,
                                ).await;
                            }
                        }
                        None => {
                            // Channel closed, flush remaining events and exit
                            if !current_batch.is_empty() {
                                Self::flush_batch(
                                    processor_id,
                                    &event_store,
                                    &mut current_batch,
                                    &metrics,
                                    &config,
                                ).await;
                            }
                            break;
                        }
                    }
                }
                
                // Timer tick - flush if batch has expired
                _ = batch_timer.tick() => {
                    if !current_batch.is_empty() && current_batch.is_expired(config.max_batch_delay) {
                        Self::flush_batch(
                            processor_id,
                            &event_store,
                            &mut current_batch,
                            &metrics,
                            &config,
                        ).await;
                    }
                }
                
                // Shutdown signal
                _ = shutdown_notify.notified() => {
                    if !current_batch.is_empty() {
                        Self::flush_batch(
                            processor_id,
                            &event_store,
                            &mut current_batch,
                            &metrics,
                            &config,
                        ).await;
                    }
                    break;
                }
            }
        }
        
        tracing::info!("Batch processor {} shutting down", processor_id);
    }
    
    /// Flush a batch to the event store with optimized append operations
    async fn flush_batch(
        processor_id: usize,
        event_store: &EventStore,
        batch: &mut EventBatch,
        metrics: &Arc<Mutex<BatchMetrics>>,
        config: &BatchConfig,
    ) {
        if batch.is_empty() {
            return;
        }
        
        let batch_size = batch.len();
        let start_time = Instant::now();
        
        // Use optimized append if configured
        let result = if config.use_optimized_append {
            event_store.append_events_optimized(batch.events.clone()).await
                .map(|append_result| Some(append_result))
        } else {
            event_store.append_events(batch.events.clone()).await
                .map(|_| None)
        };
        
        match result {
            Ok(append_result) => {
                let duration = start_time.elapsed();
                
                // Update metrics
                let mut metrics_guard = metrics.lock().await;
                metrics_guard.total_events_processed += batch_size as u64;
                metrics_guard.total_batches_processed += 1;
                metrics_guard.average_batch_size =
                    metrics_guard.total_events_processed as f64 / metrics_guard.total_batches_processed as f64;
                
                // Update throughput metrics
                let current_throughput = batch_size as f64 / duration.as_secs_f64();
                if current_throughput > metrics_guard.peak_throughput {
                    metrics_guard.peak_throughput = current_throughput;
                }
                
                if let Some(last_time) = metrics_guard.last_batch_time {
                    let time_diff = start_time.duration_since(last_time).as_secs_f64();
                    if time_diff > 0.0 {
                        metrics_guard.events_per_second = batch_size as f64 / time_diff;
                    }
                }
                metrics_guard.last_batch_time = Some(start_time);
                
                // Update latency metrics
                let total_latency = metrics_guard.average_write_latency.as_nanos() as f64 * metrics_guard.total_batches_processed as f64;
                let new_total_latency = total_latency + duration.as_nanos() as f64;
                metrics_guard.average_write_latency = Duration::from_nanos(
                    (new_total_latency / (metrics_guard.total_batches_processed + 1) as f64) as u64
                );
                
                // Update bytes written if available
                if let Some(append_result) = append_result {
                    metrics_guard.total_bytes_written += append_result.bytes_written as u64;
                }
                
                tracing::debug!(
                    "Processor {} flushed batch of {} events in {:?} ({:.0} events/sec)",
                    processor_id,
                    batch_size,
                    duration,
                    current_throughput
                );
            }
            Err(e) => {
                let mut metrics_guard = metrics.lock().await;
                metrics_guard.processing_errors += 1;
                
                // Check for append-only violations
                if e.to_string().contains("already exists") {
                    metrics_guard.append_only_violations += 1;
                }
                
                tracing::error!(
                    "Processor {} failed to flush batch of {} events: {}",
                    processor_id,
                    batch_size,
                    e
                );
            }
        }
        
        // Reset batch
        *batch = EventBatch::new();
    }
    
    /// Get current batch processing metrics
    pub async fn get_metrics(&self) -> BatchMetrics {
        (*self.metrics.lock().await).clone()
    }
    
    /// Shutdown the batch processor gracefully
    pub async fn shutdown(&self) {
        self.shutdown_notify.notify_waiters();
        
        // Give processors time to flush remaining batches
        sleep(self.config.max_batch_delay * 2).await;
    }
}

/// Synchronous batch writer for high-throughput scenarios
pub struct SyncBatchWriter {
    event_store: Arc<EventStore>,
    pending_events: VecDeque<EventEnvelope>,
    max_batch_size: usize,
}

impl SyncBatchWriter {
    /// Create a new synchronous batch writer
    pub fn new(event_store: Arc<EventStore>, max_batch_size: usize) -> Self {
        Self {
            event_store,
            pending_events: VecDeque::new(),
            max_batch_size,
        }
    }
    
    /// Add an event to the pending batch
    pub fn add_event(&mut self, event: EventEnvelope) {
        self.pending_events.push_back(event);
    }
    
    /// Flush pending events if batch is full
    pub async fn flush_if_full(&mut self) -> Result<bool, EventStoreError> {
        if self.pending_events.len() >= self.max_batch_size {
            self.flush().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
    
    /// Flush all pending events
    pub async fn flush(&mut self) -> Result<(), EventStoreError> {
        if self.pending_events.is_empty() {
            return Ok(());
        }
        
        let events: Vec<EventEnvelope> = self.pending_events.drain(..).collect();
        self.event_store.append_events(events).await?;
        
        Ok(())
    }
    
    /// Get the number of pending events
    pub fn pending_count(&self) -> usize {
        self.pending_events.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{config::EventStoreConfig, store::EventStore};
    use tempfile::TempDir;
    
    async fn create_test_store() -> (Arc<EventStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::default();
        let store = Arc::new(EventStore::new(temp_dir.path(), config).await.unwrap());
        (store, temp_dir)
    }
    
    fn create_test_event(account_id: AccountId) -> EventEnvelope {
        let event = AccountCreatedEvent {
            account_id: account_id.clone(),
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            initial_balance: Amount::new(1000),
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                tags: std::collections::HashMap::new(),
                regulatory_flags: vec![],
            },
            created_at: chrono::Utc::now(),
        };
        
        EventEnvelope::new(
            account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        )
    }
    
    #[tokio::test]
    async fn test_batch_creation() {
        let mut batch = EventBatch::new();
        assert!(batch.is_empty());
        
        let account_id = AccountId::new();
        let event = create_test_event(account_id);
        
        batch.add_event(event);
        assert_eq!(batch.len(), 1);
        assert!(!batch.is_empty());
        assert!(!batch.is_full(10));
    }
    
    #[tokio::test]
    async fn test_sync_batch_writer() {
        let (store, _temp_dir) = create_test_store().await;
        let mut writer = SyncBatchWriter::new(store.clone(), 3);
        
        // Add events
        for i in 0..5 {
            let account_id = AccountId::new();
            let event = create_test_event(account_id);
            writer.add_event(event);
            
            if i == 2 {
                // Should flush when we hit batch size
                let flushed = writer.flush_if_full().await.unwrap();
                assert!(flushed);
                assert_eq!(writer.pending_count(), 0);
            }
        }
        
        // Should have 2 pending events
        assert_eq!(writer.pending_count(), 2);
        
        // Manual flush
        writer.flush().await.unwrap();
        assert_eq!(writer.pending_count(), 0);
        
        // Verify events were stored
        let latest_events = store.get_latest_events(10).await.unwrap();
        assert_eq!(latest_events.len(), 5);
    }
    
    #[tokio::test]
    async fn test_batch_processor_basic() {
        let (store, _temp_dir) = create_test_store().await;
        let config = BatchConfig {
            max_batch_size: 3,
            max_batch_delay: Duration::from_millis(100),
            buffer_size: 100,
            num_processors: 1,
            use_optimized_append: true,
            target_throughput: 1000,
            enforce_append_only: true,
        };
        
        let processor = BatchProcessor::new(store.clone(), config);
        let sender = processor.start().await.unwrap();
        
        // Send events
        for _ in 0..5 {
            let account_id = AccountId::new();
            let event = create_test_event(account_id);
            sender.send(event).await.unwrap();
        }
        
        // Wait for processing
        sleep(Duration::from_millis(200)).await;
        
        // Check metrics
        let metrics = processor.get_metrics().await;
        assert!(metrics.total_events_processed >= 5);
        
        // Shutdown
        processor.shutdown().await;
        
        // Verify events were stored
        let latest_events = store.get_latest_events(10).await.unwrap();
        assert_eq!(latest_events.len(), 5);
    }
}