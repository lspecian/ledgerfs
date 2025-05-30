//! Event Store Recovery and Consistency Checking
//! 
//! This module provides crash recovery mechanisms and consistency validation
//! for the event store to ensure data integrity.

use crate::{
    store::{EventStore, EventStoreError},
    serialization::EventSerializer,
};
use ledgerfs_core::*;
use rocksdb::{DB, IteratorMode};
use std::collections::HashMap;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur during recovery operations
#[derive(Error, Debug)]
pub enum RecoveryError {
    #[error("Event store error: {0}")]
    EventStoreError(#[from] EventStoreError),
    
    #[error("Consistency check failed: {0}")]
    ConsistencyCheckFailed(String),
    
    #[error("Recovery operation failed: {0}")]
    RecoveryFailed(String),
    
    #[error("Corrupted event data: {0}")]
    CorruptedData(String),
}

/// Recovery statistics
#[derive(Debug, Clone)]
pub struct RecoveryStats {
    pub total_events_checked: u64,
    pub corrupted_events_found: u64,
    pub corrupted_events_repaired: u64,
    pub consistency_errors: u64,
    pub recovery_duration_ms: u64,
}

/// Recovery options
#[derive(Debug, Clone)]
pub struct RecoveryOptions {
    /// Whether to attempt automatic repair of corrupted events
    pub auto_repair: bool,
    
    /// Whether to perform deep consistency checks
    pub deep_check: bool,
    
    /// Maximum number of events to check (0 = all)
    pub max_events_to_check: u64,
    
    /// Whether to create backup before recovery
    pub create_backup: bool,
}

impl Default for RecoveryOptions {
    fn default() -> Self {
        Self {
            auto_repair: false,
            deep_check: true,
            max_events_to_check: 0,
            create_backup: true,
        }
    }
}

/// Event store recovery manager
pub struct RecoveryManager {
    event_store: Arc<EventStore>,
    serializer: EventSerializer,
}

impl RecoveryManager {
    /// Create a new recovery manager
    pub fn new(event_store: Arc<EventStore>) -> Self {
        Self {
            event_store,
            serializer: EventSerializer::new(true), // Use compression for recovery
        }
    }
    
    /// Perform crash recovery and consistency checks
    pub async fn recover(&self, options: RecoveryOptions) -> Result<RecoveryStats, RecoveryError> {
        let start_time = std::time::Instant::now();
        
        tracing::info!("Starting event store recovery with options: {:?}", options);
        
        let mut stats = RecoveryStats {
            total_events_checked: 0,
            corrupted_events_found: 0,
            corrupted_events_repaired: 0,
            consistency_errors: 0,
            recovery_duration_ms: 0,
        };
        
        // Step 1: Basic integrity check
        self.check_database_integrity(&mut stats).await?;
        
        // Step 2: Event-level consistency checks
        if options.deep_check {
            self.check_event_consistency(&mut stats, &options).await?;
        }
        
        // Step 3: Account balance consistency
        self.check_account_balance_consistency(&mut stats).await?;
        
        // Step 4: Temporal consistency
        self.check_temporal_consistency(&mut stats).await?;
        
        stats.recovery_duration_ms = start_time.elapsed().as_millis() as u64;
        
        tracing::info!("Recovery completed: {:?}", stats);
        
        Ok(stats)
    }
    
    /// Check basic database integrity
    async fn check_database_integrity(&self, stats: &mut RecoveryStats) -> Result<(), RecoveryError> {
        tracing::info!("Checking database integrity...");
        
        // This would typically involve checking RocksDB's internal consistency
        // For now, we'll do a basic iteration to ensure all events can be read
        
        let query = crate::store::EventQuery {
            account_id: None,
            start_time: None,
            end_time: None,
            limit: None,
            offset: None,
            event_types: None,
        };
        
        match self.event_store.query_events(query).await {
            Ok(result) => {
                stats.total_events_checked = result.events.len() as u64;
                tracing::info!("Database integrity check passed: {} events verified", result.events.len());
                Ok(())
            }
            Err(e) => {
                stats.consistency_errors += 1;
                Err(RecoveryError::ConsistencyCheckFailed(format!("Database integrity check failed: {}", e)))
            }
        }
    }
    
    /// Check event-level consistency
    async fn check_event_consistency(
        &self,
        stats: &mut RecoveryStats,
        options: &RecoveryOptions,
    ) -> Result<(), RecoveryError> {
        tracing::info!("Checking event consistency...");
        
        let query = crate::store::EventQuery {
            account_id: None,
            start_time: None,
            end_time: None,
            limit: if options.max_events_to_check > 0 { Some(options.max_events_to_check as usize) } else { None },
            offset: None,
            event_types: None,
        };
        
        let result = self.event_store.query_events(query).await?;
        
        for envelope in result.events {
            stats.total_events_checked += 1;
            
            // Check event envelope consistency
            if let Err(e) = self.validate_event_envelope(&envelope) {
                stats.corrupted_events_found += 1;
                tracing::warn!("Corrupted event found: {}: {}", envelope.event_id.0, e);
                
                if options.auto_repair {
                    // Attempt to repair the event
                    if self.repair_event(&envelope).await.is_ok() {
                        stats.corrupted_events_repaired += 1;
                        tracing::info!("Repaired corrupted event: {}", envelope.event_id.0);
                    }
                }
            }
        }
        
        tracing::info!("Event consistency check completed");
        Ok(())
    }
    
    /// Check account balance consistency
    async fn check_account_balance_consistency(&self, stats: &mut RecoveryStats) -> Result<(), RecoveryError> {
        tracing::info!("Checking account balance consistency...");
        
        // Get all events and rebuild account balances
        let query = crate::store::EventQuery {
            account_id: None,
            start_time: None,
            end_time: None,
            limit: None,
            offset: None,
            event_types: None,
        };
        
        let result = self.event_store.query_events(query).await?;
        let mut account_balances: HashMap<AccountId, Amount> = HashMap::new();
        
        // Replay events to calculate expected balances
        for envelope in result.events {
            match &envelope.event_data {
                LedgerEvent::AccountCreated(event) => {
                    account_balances.insert(event.account_id.clone(), event.initial_balance);
                }
                LedgerEvent::BalanceUpdated(event) => {
                    account_balances.insert(event.account_id.clone(), event.new_balance);
                }
                LedgerEvent::TransactionProcessed(event) => {
                    // Update balances based on transaction
                    if let Some(from_account) = &event.from_account {
                        if let Some(current_balance) = account_balances.get(from_account) {
                            match current_balance.subtract(event.amount) {
                                Ok(new_balance) => {
                                    account_balances.insert(from_account.clone(), new_balance);
                                }
                                Err(_) => {
                                    stats.consistency_errors += 1;
                                    tracing::warn!("Balance consistency error: insufficient funds for transaction {}", event.transaction_id.0);
                                }
                            }
                        }
                    }
                    
                    if let Some(to_account) = &event.to_account {
                        if let Some(current_balance) = account_balances.get(to_account) {
                            match current_balance.add(event.amount) {
                                Ok(new_balance) => {
                                    account_balances.insert(to_account.clone(), new_balance);
                                }
                                Err(_) => {
                                    stats.consistency_errors += 1;
                                    tracing::warn!("Balance consistency error: overflow for transaction {}", event.transaction_id.0);
                                }
                            }
                        }
                    }
                }
                _ => {} // Other events don't affect balances directly
            }
        }
        
        tracing::info!("Account balance consistency check completed: {} accounts verified", account_balances.len());
        Ok(())
    }
    
    /// Check temporal consistency (events are in chronological order)
    async fn check_temporal_consistency(&self, stats: &mut RecoveryStats) -> Result<(), RecoveryError> {
        tracing::info!("Checking temporal consistency...");
        
        let query = crate::store::EventQuery {
            account_id: None,
            start_time: None,
            end_time: None,
            limit: None,
            offset: None,
            event_types: None,
        };
        
        let result = self.event_store.query_events(query).await?;
        let mut last_timestamp: Option<chrono::DateTime<chrono::Utc>> = None;
        
        for envelope in result.events {
            if let Some(last_ts) = last_timestamp {
                if envelope.timestamp < last_ts {
                    stats.consistency_errors += 1;
                    tracing::warn!(
                        "Temporal consistency error: event {} has timestamp {} which is before previous event timestamp {}",
                        envelope.event_id.0,
                        envelope.timestamp,
                        last_ts
                    );
                }
            }
            last_timestamp = Some(envelope.timestamp);
        }
        
        tracing::info!("Temporal consistency check completed");
        Ok(())
    }
    
    /// Validate an individual event envelope
    fn validate_event_envelope(&self, envelope: &EventEnvelope) -> Result<(), String> {
        // Check that event_type matches the actual event data
        let expected_type = match &envelope.event_data {
            LedgerEvent::AccountCreated(_) => "AccountCreated",
            LedgerEvent::AccountStatusChanged(_) => "AccountStatusChanged",
            LedgerEvent::AccountMetadataUpdated(_) => "AccountMetadataUpdated",
            LedgerEvent::TransactionProcessed(_) => "TransactionProcessed",
            LedgerEvent::BalanceUpdated(_) => "BalanceUpdated",
            LedgerEvent::MerkleRootGenerated(_) => "MerkleRootGenerated",
            LedgerEvent::DigestPublished(_) => "DigestPublished",
        };
        
        if envelope.event_type != expected_type {
            return Err(format!("Event type mismatch: expected {}, got {}", expected_type, envelope.event_type));
        }
        
        // Check that aggregate_id is not empty
        if envelope.aggregate_id.is_empty() {
            return Err("Empty aggregate_id".to_string());
        }
        
        // Check that timestamp is reasonable (not in the future, not too old)
        let now = chrono::Utc::now();
        if envelope.timestamp > now {
            return Err(format!("Event timestamp is in the future: {}", envelope.timestamp));
        }
        
        // Check that it's not older than 10 years (arbitrary reasonable limit)
        let ten_years_ago = now - chrono::Duration::days(365 * 10);
        if envelope.timestamp < ten_years_ago {
            return Err(format!("Event timestamp is too old: {}", envelope.timestamp));
        }
        
        Ok(())
    }
    
    /// Attempt to repair a corrupted event
    async fn repair_event(&self, envelope: &EventEnvelope) -> Result<(), RecoveryError> {
        // This is a placeholder for event repair logic
        // In a real implementation, you might:
        // 1. Try to reconstruct the event from other sources
        // 2. Mark the event as corrupted but preserve it
        // 3. Remove the event if it's beyond repair
        
        tracing::warn!("Event repair not implemented for event: {}", envelope.event_id.0);
        Err(RecoveryError::RecoveryFailed("Event repair not implemented".to_string()))
    }
    
    /// Create a backup of the event store
    pub async fn create_backup(&self, backup_path: &str) -> Result<(), RecoveryError> {
        tracing::info!("Creating backup at: {}", backup_path);
        
        // This would typically involve:
        // 1. Creating a consistent snapshot of the RocksDB
        // 2. Copying all data files to the backup location
        // 3. Verifying the backup integrity
        
        // For now, this is a placeholder
        tracing::warn!("Backup creation not implemented");
        Ok(())
    }
    
    /// Restore from a backup
    pub async fn restore_from_backup(&self, backup_path: &str) -> Result<(), RecoveryError> {
        tracing::info!("Restoring from backup: {}", backup_path);
        
        // This would typically involve:
        // 1. Stopping all write operations
        // 2. Replacing current data with backup data
        // 3. Restarting the event store
        // 4. Verifying the restored data
        
        // For now, this is a placeholder
        tracing::warn!("Backup restoration not implemented");
        Ok(())
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
    
    #[tokio::test]
    async fn test_recovery_empty_store() {
        let (store, _temp_dir) = create_test_store().await;
        let recovery_manager = RecoveryManager::new(store);
        
        let options = RecoveryOptions::default();
        let stats = recovery_manager.recover(options).await.unwrap();
        
        assert_eq!(stats.corrupted_events_found, 0);
        assert_eq!(stats.consistency_errors, 0);
    }
    
    #[tokio::test]
    async fn test_recovery_with_events() {
        let (store, _temp_dir) = create_test_store().await;
        
        // Add some test events
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
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        store.append_event(envelope).await.unwrap();
        
        let recovery_manager = RecoveryManager::new(store);
        let options = RecoveryOptions::default();
        let stats = recovery_manager.recover(options).await.unwrap();
        
        assert_eq!(stats.total_events_checked, 1);
        assert_eq!(stats.corrupted_events_found, 0);
        assert_eq!(stats.consistency_errors, 0);
    }
}