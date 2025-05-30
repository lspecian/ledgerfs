use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::types::*;

/// Base event trait for all ledger events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventEnvelope {
    pub event_id: EventId,
    pub event_type: String,
    pub aggregate_id: String,
    pub aggregate_version: u64,
    pub timestamp: DateTime<Utc>,
    pub event_data: LedgerEvent,
    pub metadata: EventMetadata,
}

/// Metadata for events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMetadata {
    pub correlation_id: Option<String>,
    pub causation_id: Option<String>,
    pub user_id: Option<String>,
    pub source: String,
}

/// All possible ledger events
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum LedgerEvent {
    AccountCreated(AccountCreatedEvent),
    AccountStatusChanged(AccountStatusChangedEvent),
    AccountMetadataUpdated(AccountMetadataUpdatedEvent),
    TransactionProcessed(TransactionProcessedEvent),
    BalanceUpdated(BalanceUpdatedEvent),
    MerkleRootGenerated(MerkleRootGeneratedEvent),
    DigestPublished(DigestPublishedEvent),
}

/// Account creation event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountCreatedEvent {
    pub account_id: AccountId,
    pub account_type: AccountType,
    pub currency: Currency,
    pub initial_balance: Amount,
    pub metadata: AccountMetadata,
    pub created_at: DateTime<Utc>,
}

/// Account status change event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountStatusChangedEvent {
    pub account_id: AccountId,
    pub old_status: AccountStatus,
    pub new_status: AccountStatus,
    pub reason: Option<String>,
    pub changed_at: DateTime<Utc>,
}

/// Account metadata update event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMetadataUpdatedEvent {
    pub account_id: AccountId,
    pub old_metadata: AccountMetadata,
    pub new_metadata: AccountMetadata,
    pub updated_at: DateTime<Utc>,
}

/// Transaction processed event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionProcessedEvent {
    pub transaction_id: TransactionId,
    pub transaction_type: TransactionType,
    pub from_account: Option<AccountId>,
    pub to_account: Option<AccountId>,
    pub amount: Amount,
    pub currency: Currency,
    pub metadata: TransactionMetadata,
    pub processed_at: DateTime<Utc>,
}

/// Balance update event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalanceUpdatedEvent {
    pub account_id: AccountId,
    pub old_balance: Amount,
    pub new_balance: Amount,
    pub currency: Currency,
    pub transaction_id: Option<TransactionId>,
    pub updated_at: DateTime<Utc>,
}

/// Merkle root generation event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleRootGeneratedEvent {
    pub block_height: u64,
    pub root_hash: String,
    pub transaction_count: u64,
    pub previous_root_hash: Option<String>,
    pub generated_at: DateTime<Utc>,
}

/// Digest publication event
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DigestPublishedEvent {
    pub digest: LedgerDigest,
    pub storage_location: String,
    pub published_at: DateTime<Utc>,
}

impl EventEnvelope {
    pub fn new(
        aggregate_id: String,
        aggregate_version: u64,
        event_data: LedgerEvent,
        metadata: EventMetadata,
    ) -> Self {
        let event_type = match &event_data {
            LedgerEvent::AccountCreated(_) => "AccountCreated",
            LedgerEvent::AccountStatusChanged(_) => "AccountStatusChanged",
            LedgerEvent::AccountMetadataUpdated(_) => "AccountMetadataUpdated",
            LedgerEvent::TransactionProcessed(_) => "TransactionProcessed",
            LedgerEvent::BalanceUpdated(_) => "BalanceUpdated",
            LedgerEvent::MerkleRootGenerated(_) => "MerkleRootGenerated",
            LedgerEvent::DigestPublished(_) => "DigestPublished",
        };

        Self {
            event_id: EventId::new(),
            event_type: event_type.to_string(),
            aggregate_id,
            aggregate_version,
            timestamp: Utc::now(),
            event_data,
            metadata,
        }
    }

    /// Calculate SHA-256 hash of the event for Merkle tree construction
    pub fn calculate_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        
        let serialized = serde_json::to_string(self)
            .expect("Event should always be serializable");
        
        let mut hasher = Sha256::new();
        hasher.update(serialized.as_bytes());
        hex::encode(hasher.finalize())
    }
}

impl Default for EventMetadata {
    fn default() -> Self {
        Self {
            correlation_id: None,
            causation_id: None,
            user_id: None,
            source: "ledgerfs".to_string(),
        }
    }
}