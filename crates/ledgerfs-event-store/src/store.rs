//! Event Store Implementation
//! 
//! This module provides the main EventStore implementation using RocksDB
//! as the underlying LSM tree storage engine.

use crate::{
    config::EventStoreConfig,
    serialization::{EventSerializer, SerializationError, generate_event_key, generate_account_key_prefix, generate_time_key_prefix},
};
use ledgerfs_core::*;
use rocksdb::{DB, Options, WriteBatch, IteratorMode, Direction};
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

/// Errors that can occur in the event store
#[derive(Error, Debug)]
pub enum EventStoreError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] rocksdb::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] SerializationError),
    
    #[error("Event not found: {0}")]
    EventNotFound(String),
    
    #[error("Invalid query parameters: {0}")]
    InvalidQuery(String),
    
    #[error("Concurrency error: {0}")]
    ConcurrencyError(String),
}

/// Query parameters for retrieving events
#[derive(Debug, Clone)]
pub struct EventQuery {
    /// Filter by account ID
    pub account_id: Option<AccountId>,
    
    /// Start time for time range queries
    pub start_time: Option<chrono::DateTime<chrono::Utc>>,
    
    /// End time for time range queries
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    
    /// Maximum number of events to return
    pub limit: Option<usize>,
    
    /// Offset for pagination
    pub offset: Option<usize>,
    
    /// Event types to filter by
    pub event_types: Option<Vec<String>>,
}

/// Result of an event query
#[derive(Debug, Clone)]
pub struct EventQueryResult {
    pub events: Vec<EventEnvelope>,
    pub total_count: Option<usize>,
    pub has_more: bool,
}

/// High-performance event store using RocksDB
pub struct EventStore {
    db: Arc<DB>,
    serializer: EventSerializer,
    config: EventStoreConfig,
    write_lock: Arc<RwLock<()>>,
}

impl EventStore {
    /// Create a new event store
    pub async fn new<P: AsRef<Path>>(
        path: P,
        config: EventStoreConfig,
    ) -> Result<Self, EventStoreError> {
        let mut opts = Options::default();
        opts.create_if_missing(true);
        opts.create_missing_column_families(true);
        
        // Apply performance optimizations from config
        opts.set_max_write_buffer_number(config.max_write_buffer_number);
        opts.set_write_buffer_size(config.write_buffer_size);
        opts.set_target_file_size_base(config.target_file_size_base);
        opts.set_max_background_jobs(config.max_background_jobs);
        
        // Set compression
        opts.set_compression_type(config.compression_type.into());
        
        // Enable WAL and sync settings
        opts.set_use_fsync(config.sync_writes);
        
        // Enable bloom filters for better read performance
        let mut block_opts = rocksdb::BlockBasedOptions::default();
        block_opts.set_bloom_filter(10.0, false);
        opts.set_block_based_table_factory(&block_opts);
        
        let db = DB::open(&opts, path)?;
        
        Ok(Self {
            db: Arc::new(db),
            serializer: EventSerializer::new(config.enable_compression),
            config,
            write_lock: Arc::new(RwLock::new(())),
        })
    }
    
    /// Append a single event to the store (append-only, immutable)
    pub async fn append_event(&self, envelope: EventEnvelope) -> Result<(), EventStoreError> {
        let _write_guard = self.write_lock.write().await;
        
        let key = generate_event_key(&envelope);
        
        // Enforce append-only semantics: check if event already exists
        if self.db.get(&key)?.is_some() {
            return Err(EventStoreError::ConcurrencyError(
                format!("Event with key {:?} already exists. Append-only store prevents overwrites.",
                    String::from_utf8_lossy(&key))
            ));
        }
        
        let value = self.serializer.serialize(&envelope)?;
        
        // Use atomic write with durability guarantees
        let mut write_opts = rocksdb::WriteOptions::default();
        write_opts.set_sync(self.config.sync_writes);
        write_opts.disable_wal(false); // Ensure WAL is enabled for durability
        
        self.db.put_opt(&key, &value, &write_opts)?;
        
        Ok(())
    }
    
    /// Append multiple events atomically with append-only guarantees
    pub async fn append_events(&self, envelopes: Vec<EventEnvelope>) -> Result<(), EventStoreError> {
        if envelopes.is_empty() {
            return Ok(());
        }
        
        let _write_guard = self.write_lock.write().await;
        
        let mut batch = WriteBatch::default();
        let mut keys_to_check = Vec::new();
        
        // Pre-serialize all events and prepare keys
        for envelope in &envelopes {
            let key = generate_event_key(envelope);
            keys_to_check.push(key.clone());
            
            // Enforce append-only semantics: check if any event already exists
            if self.db.get(&key)?.is_some() {
                return Err(EventStoreError::ConcurrencyError(
                    format!("Event with key {:?} already exists. Append-only store prevents overwrites.",
                        String::from_utf8_lossy(&key))
                ));
            }
            
            let value = self.serializer.serialize(envelope)?;
            batch.put(&key, &value);
        }
        
        // Atomic batch write with durability guarantees
        let mut write_opts = rocksdb::WriteOptions::default();
        write_opts.set_sync(self.config.sync_writes);
        write_opts.disable_wal(false); // Ensure WAL is enabled for durability
        
        self.db.write_opt(batch, &write_opts)?;
        
        Ok(())
    }
    
    /// Get a specific event by its ID
    pub async fn get_event(&self, event_id: &EventId) -> Result<Option<EventEnvelope>, EventStoreError> {
        let _read_guard = self.write_lock.read().await;
        
        // Since we don't store by event_id directly, we need to scan
        // In a production system, we'd maintain a secondary index
        let iter = self.db.iterator(IteratorMode::Start);
        
        for item in iter {
            let (_, value) = item?;
            let envelope = self.serializer.deserialize(&value)?;
            
            if envelope.event_id == *event_id {
                return Ok(Some(envelope));
            }
        }
        
        Ok(None)
    }
    
    /// Query events based on criteria
    pub async fn query_events(&self, query: EventQuery) -> Result<EventQueryResult, EventStoreError> {
        let _read_guard = self.write_lock.read().await;
        
        let mut events = Vec::new();
        let mut count = 0;
        let limit = query.limit.unwrap_or(1000);
        let offset = query.offset.unwrap_or(0);
        
        // Determine iteration strategy based on query
        let iter = if let Some(start_time) = query.start_time {
            let start_key = generate_time_key_prefix(start_time);
            self.db.iterator(IteratorMode::From(&start_key, Direction::Forward))
        } else {
            self.db.iterator(IteratorMode::Start)
        };
        
        for item in iter {
            let (_, value) = item?;
            let envelope = self.serializer.deserialize(&value)?;
            
            // Apply filters
            if let Some(account_id) = &query.account_id {
                if !self.event_matches_account(&envelope, account_id) {
                    continue;
                }
            }
            
            if let Some(end_time) = query.end_time {
                if envelope.timestamp > end_time {
                    break;
                }
            }
            
            if let Some(event_types) = &query.event_types {
                if !event_types.contains(&envelope.event_type) {
                    continue;
                }
            }
            
            // Apply pagination
            if count < offset {
                count += 1;
                continue;
            }
            
            if events.len() >= limit {
                break;
            }
            
            events.push(envelope);
            count += 1;
        }
        
        let has_more = events.len() == limit;
        let events_len = events.len();
        
        Ok(EventQueryResult {
            events,
            total_count: None, // Would require a separate count query
            has_more,
        })
    }
    
    /// Get all events for a specific account
    pub async fn get_account_events(&self, account_id: &AccountId) -> Result<Vec<EventEnvelope>, EventStoreError> {
        let query = EventQuery {
            account_id: Some(account_id.clone()),
            start_time: None,
            end_time: None,
            limit: None,
            offset: None,
            event_types: None,
        };
        
        let result = self.query_events(query).await?;
        Ok(result.events)
    }
    
    /// Get events in a time range
    pub async fn get_events_in_range(
        &self,
        start_time: chrono::DateTime<chrono::Utc>,
        end_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<EventEnvelope>, EventStoreError> {
        let query = EventQuery {
            account_id: None,
            start_time: Some(start_time),
            end_time: Some(end_time),
            limit: None,
            offset: None,
            event_types: None,
        };
        
        let result = self.query_events(query).await?;
        Ok(result.events)
    }
    
    /// Get the latest events (most recent first)
    pub async fn get_latest_events(&self, limit: usize) -> Result<Vec<EventEnvelope>, EventStoreError> {
        let _read_guard = self.write_lock.read().await;
        
        let mut events = Vec::new();
        let iter = self.db.iterator(IteratorMode::End);
        
        for item in iter.take(limit) {
            let (_, value) = item?;
            let envelope = self.serializer.deserialize(&value)?;
            events.push(envelope);
        }
        
        // Reverse to get chronological order (oldest first)
        events.reverse();
        Ok(events)
    }
    
    /// Get statistics about the event store
    pub async fn get_statistics(&self) -> Result<EventStoreStatistics, EventStoreError> {
        let _read_guard = self.write_lock.read().await;
        
        let mut total_events = 0;
        let mut oldest_event: Option<chrono::DateTime<chrono::Utc>> = None;
        let mut newest_event: Option<chrono::DateTime<chrono::Utc>> = None;
        
        let iter = self.db.iterator(IteratorMode::Start);
        
        for item in iter {
            let (_, value) = item?;
            let envelope = self.serializer.deserialize(&value)?;
            
            total_events += 1;
            
            if oldest_event.is_none() || envelope.timestamp < oldest_event.unwrap() {
                oldest_event = Some(envelope.timestamp);
            }
            
            if newest_event.is_none() || envelope.timestamp > newest_event.unwrap() {
                newest_event = Some(envelope.timestamp);
            }
        }
        
        Ok(EventStoreStatistics {
            total_events,
            oldest_event,
            newest_event,
            database_size_bytes: self.get_database_size()?,
        })
    }
    
    /// Append events with optimized batching for high throughput
    pub async fn append_events_optimized(&self, envelopes: Vec<EventEnvelope>) -> Result<AppendResult, EventStoreError> {
        if envelopes.is_empty() {
            return Ok(AppendResult {
                events_written: 0,
                bytes_written: 0,
                write_duration: std::time::Duration::ZERO,
            });
        }
        
        let start_time = std::time::Instant::now();
        let _write_guard = self.write_lock.write().await;
        
        let mut batch = WriteBatch::default();
        let mut total_bytes = 0;
        
        // Pre-validate all events for append-only semantics
        for envelope in &envelopes {
            let key = generate_event_key(envelope);
            
            // Check for existing events (append-only enforcement)
            if self.db.get(&key)?.is_some() {
                return Err(EventStoreError::ConcurrencyError(
                    format!("Event with key {:?} already exists. Append-only store prevents overwrites.",
                        String::from_utf8_lossy(&key))
                ));
            }
        }
        
        // Serialize and batch all events
        for envelope in &envelopes {
            let key = generate_event_key(envelope);
            let value = self.serializer.serialize(envelope)?;
            total_bytes += key.len() + value.len();
            batch.put(&key, &value);
        }
        
        // Optimized write options for high throughput
        let mut write_opts = rocksdb::WriteOptions::default();
        write_opts.set_sync(self.config.sync_writes);
        write_opts.disable_wal(false);
        
        // Use LSM tree optimizations
        if envelopes.len() > 100 {
            // For large batches, disable sync for better throughput
            // WAL still provides durability
            write_opts.set_sync(false);
        }
        
        self.db.write_opt(batch, &write_opts)?;
        
        let write_duration = start_time.elapsed();
        
        Ok(AppendResult {
            events_written: envelopes.len(),
            bytes_written: total_bytes,
            write_duration,
        })
    }
    
    /// Compact the database to reclaim space
    pub async fn compact(&self) -> Result<(), EventStoreError> {
        let _write_guard = self.write_lock.write().await;
        
        self.db.compact_range(None::<&[u8]>, None::<&[u8]>);
        Ok(())
    }
    
    /// Helper method to check if an event matches an account
    fn event_matches_account(&self, envelope: &EventEnvelope, account_id: &AccountId) -> bool {
        match &envelope.event_data {
            LedgerEvent::AccountCreated(event) => event.account_id == *account_id,
            LedgerEvent::AccountStatusChanged(event) => event.account_id == *account_id,
            LedgerEvent::AccountMetadataUpdated(event) => event.account_id == *account_id,
            LedgerEvent::TransactionProcessed(event) => {
                event.from_account == Some(account_id.clone()) || 
                event.to_account == Some(account_id.clone())
            },
            LedgerEvent::BalanceUpdated(event) => event.account_id == *account_id,
            _ => false,
        }
    }
    
    /// Get database size in bytes
    fn get_database_size(&self) -> Result<u64, EventStoreError> {
        // This is a simplified implementation
        // In production, you'd want to use RocksDB's property API
        Ok(0)
    }
}

/// Result of an append operation
#[derive(Debug, Clone)]
pub struct AppendResult {
    pub events_written: usize,
    pub bytes_written: usize,
    pub write_duration: std::time::Duration,
}

/// Statistics about the event store
#[derive(Debug, Clone)]
pub struct EventStoreStatistics {
    pub total_events: u64,
    pub oldest_event: Option<chrono::DateTime<chrono::Utc>>,
    pub newest_event: Option<chrono::DateTime<chrono::Utc>>,
    pub database_size_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    async fn create_test_store() -> (EventStore, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let config = EventStoreConfig::default();
        let store = EventStore::new(temp_dir.path(), config).await.unwrap();
        (store, temp_dir)
    }
    
    #[tokio::test]
    async fn test_append_and_retrieve_event() {
        let (store, _temp_dir) = create_test_store().await;
        
        let event = AccountCreatedEvent {
            account_id: AccountId::new(),
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
        
        let envelope = EventEnvelope::new(
            event.account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event.clone()),
            EventMetadata::default(),
        );
        
        // Append event
        store.append_event(envelope.clone()).await.unwrap();
        
        // Retrieve by account
        let account_events = store.get_account_events(&event.account_id).await.unwrap();
        assert_eq!(account_events.len(), 1);
        assert_eq!(account_events[0].event_id, envelope.event_id);
    }
    
    #[tokio::test]
    async fn test_batch_append() {
        let (store, _temp_dir) = create_test_store().await;
        
        let mut envelopes = Vec::new();
        
        for i in 0..10 {
            let event = AccountCreatedEvent {
                account_id: AccountId::new(),
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                initial_balance: Amount::new(1000 + i),
                metadata: AccountMetadata {
                    account_type: AccountType::Customer,
                    currency: Currency::usd(),
                    tags: std::collections::HashMap::new(),
                    regulatory_flags: vec![],
                },
                created_at: chrono::Utc::now(),
            };
            
            let envelope = EventEnvelope::new(
                event.account_id.0.to_string(),
                1,
                LedgerEvent::AccountCreated(event),
                EventMetadata::default(),
            );
            
            envelopes.push(envelope);
        }
        
        // Batch append
        store.append_events(envelopes.clone()).await.unwrap();
        
        // Verify all events were stored
        let latest_events = store.get_latest_events(20).await.unwrap();
        assert_eq!(latest_events.len(), 10);
    }
    
    #[tokio::test]
    async fn test_query_events() {
        let (store, _temp_dir) = create_test_store().await;
        
        let account_id = AccountId::new();
        
        // Create multiple events for the same account
        for i in 0..5 {
            let event = AccountCreatedEvent {
                account_id: account_id.clone(),
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                initial_balance: Amount::new(1000 + i),
                metadata: AccountMetadata {
                    account_type: AccountType::Customer,
                    currency: Currency::usd(),
                    tags: std::collections::HashMap::new(),
                    regulatory_flags: vec![],
                },
                created_at: chrono::Utc::now(),
            };
            
            let envelope = EventEnvelope::new(
                event.account_id.0.to_string(),
                1,
                LedgerEvent::AccountCreated(event),
                EventMetadata::default(),
            );
            
            store.append_event(envelope).await.unwrap();
        }
        
        // Query events for the account
        let query = EventQuery {
            account_id: Some(account_id.clone()),
            start_time: None,
            end_time: None,
            limit: Some(3),
            offset: None,
            event_types: None,
        };
        
        let result = store.query_events(query).await.unwrap();
        assert_eq!(result.events.len(), 3);
        assert!(result.has_more);
    }
}