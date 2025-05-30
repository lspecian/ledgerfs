use async_trait::async_trait;
use crate::{events::*, commands::*, errors::*, types::*};
use std::collections::HashMap;

/// Event store trait for persisting and retrieving events
#[async_trait]
pub trait EventStore: Send + Sync {
    /// Append events to the store
    async fn append_events(
        &self,
        aggregate_id: &str,
        expected_version: Option<u64>,
        events: Vec<EventEnvelope>,
    ) -> LedgerResult<()>;

    /// Get events for an aggregate
    async fn get_events(
        &self,
        aggregate_id: &str,
        from_version: Option<u64>,
    ) -> LedgerResult<Vec<EventEnvelope>>;

    /// Get events by time range
    async fn get_events_by_time_range(
        &self,
        from: chrono::DateTime<chrono::Utc>,
        to: chrono::DateTime<chrono::Utc>,
    ) -> LedgerResult<Vec<EventEnvelope>>;

    /// Get all events from a specific version
    async fn get_all_events_from_version(&self, version: u64) -> LedgerResult<Vec<EventEnvelope>>;

    /// Get the current version of an aggregate
    async fn get_current_version(&self, aggregate_id: &str) -> LedgerResult<Option<u64>>;
}

/// Command handler trait
#[async_trait]
pub trait CommandHandler<C>: Send + Sync {
    /// Handle a command and return resulting events
    async fn handle(&self, command: C) -> LedgerResult<Vec<EventEnvelope>>;
}

/// Event handler trait for projections
#[async_trait]
pub trait EventHandler<E>: Send + Sync {
    /// Handle an event
    async fn handle(&self, event: E) -> LedgerResult<()>;
}

/// Read model repository trait
#[async_trait]
pub trait ReadModelRepository<T>: Send + Sync {
    /// Get by ID
    async fn get_by_id(&self, id: &str) -> LedgerResult<Option<T>>;

    /// Save or update
    async fn save(&self, entity: T) -> LedgerResult<()>;

    /// Delete by ID
    async fn delete(&self, id: &str) -> LedgerResult<()>;

    /// Find with criteria
    async fn find(&self, criteria: HashMap<String, String>) -> LedgerResult<Vec<T>>;
}

/// Balance repository trait
#[async_trait]
pub trait BalanceRepository: Send + Sync {
    /// Get current balance for an account
    async fn get_balance(&self, account_id: &AccountId) -> LedgerResult<Option<Balance>>;

    /// Update balance
    async fn update_balance(&self, balance: Balance) -> LedgerResult<()>;

    /// Get balances for multiple accounts
    async fn get_balances(&self, account_ids: &[AccountId]) -> LedgerResult<Vec<Balance>>;
}

/// Account repository trait
#[async_trait]
pub trait AccountRepository: Send + Sync {
    /// Get account by ID
    async fn get_account(&self, account_id: &AccountId) -> LedgerResult<Option<Account>>;

    /// Save account
    async fn save_account(&self, account: Account) -> LedgerResult<()>;

    /// Find accounts by criteria
    async fn find_accounts(&self, criteria: HashMap<String, String>) -> LedgerResult<Vec<Account>>;
}

/// Cryptographic service trait
#[async_trait]
pub trait CryptographicService: Send + Sync {
    /// Calculate hash of data
    fn calculate_hash(&self, data: &[u8]) -> String;

    /// Build Merkle tree from hashes
    fn build_merkle_tree(&self, hashes: Vec<String>) -> LedgerResult<MerkleNode>;

    /// Generate inclusion proof
    fn generate_inclusion_proof(
        &self,
        tree: &MerkleNode,
        leaf_hash: &str,
    ) -> LedgerResult<MerkleProof>;

    /// Verify inclusion proof
    fn verify_inclusion_proof(&self, proof: &MerkleProof) -> LedgerResult<bool>;

    /// Generate digest
    async fn generate_digest(
        &self,
        root_hash: String,
        block_height: u64,
        transaction_count: u64,
    ) -> LedgerResult<LedgerDigest>;
}

/// External storage trait for digests
#[async_trait]
pub trait ExternalStorage: Send + Sync {
    /// Store digest
    async fn store_digest(&self, digest: &LedgerDigest) -> LedgerResult<String>;

    /// Retrieve digest
    async fn retrieve_digest(&self, location: &str) -> LedgerResult<LedgerDigest>;

    /// List stored digests
    async fn list_digests(&self) -> LedgerResult<Vec<String>>;
}

/// Event streaming trait
#[async_trait]
pub trait EventStreaming: Send + Sync {
    /// Publish event to stream
    async fn publish_event(&self, event: &EventEnvelope) -> LedgerResult<()>;

    /// Subscribe to events
    async fn subscribe<F>(&self, handler: F) -> LedgerResult<()>
    where
        F: Fn(EventEnvelope) -> LedgerResult<()> + Send + Sync + 'static;
}

/// Snapshot store trait for performance optimization
#[async_trait]
pub trait SnapshotStore: Send + Sync {
    /// Save snapshot
    async fn save_snapshot<T>(&self, aggregate_id: &str, version: u64, snapshot: T) -> LedgerResult<()>
    where
        T: serde::Serialize + Send;

    /// Load snapshot
    async fn load_snapshot<T>(&self, aggregate_id: &str) -> LedgerResult<Option<(u64, T)>>
    where
        T: serde::de::DeserializeOwned + Send;

    /// Delete snapshot
    async fn delete_snapshot(&self, aggregate_id: &str) -> LedgerResult<()>;
}

/// Query service trait for complex queries
#[async_trait]
pub trait QueryService: Send + Sync {
    /// Get account balance at specific point in time
    async fn get_balance_at_time(
        &self,
        account_id: &AccountId,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> LedgerResult<Option<Balance>>;

    /// Get transaction history for account
    async fn get_transaction_history(
        &self,
        account_id: &AccountId,
        from: Option<chrono::DateTime<chrono::Utc>>,
        to: Option<chrono::DateTime<chrono::Utc>>,
        limit: Option<usize>,
    ) -> LedgerResult<Vec<TransactionProcessedEvent>>;

    /// Search transactions by criteria
    async fn search_transactions(
        &self,
        criteria: HashMap<String, String>,
        limit: Option<usize>,
    ) -> LedgerResult<Vec<TransactionProcessedEvent>>;
}

/// FUSE filesystem trait
#[async_trait]
pub trait FuseFilesystem: Send + Sync {
    /// Read file content
    async fn read_file(&self, path: &str) -> crate::errors::FuseResult<Vec<u8>>;

    /// List directory contents
    async fn list_directory(&self, path: &str) -> crate::errors::FuseResult<Vec<String>>;

    /// Get file attributes
    async fn get_file_attributes(&self, path: &str) -> crate::errors::FuseResult<FileAttributes>;
}

/// File attributes for FUSE
#[derive(Debug, Clone)]
pub struct FileAttributes {
    pub size: u64,
    pub is_directory: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub modified_at: chrono::DateTime<chrono::Utc>,
    pub permissions: u32,
}