use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Unique identifier for an account
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub Uuid);

impl AccountId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for AccountId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for a transaction
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TransactionId(pub Uuid);

impl TransactionId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for TransactionId {
    fn default() -> Self {
        Self::new()
    }
}

/// Unique identifier for an event
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EventId(pub Uuid);

impl EventId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for EventId {
    fn default() -> Self {
        Self::new()
    }
}

/// Monetary amount with precision handling
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Amount {
    /// Amount in smallest currency unit (e.g., cents for USD)
    pub value: i64,
}

impl Amount {
    pub fn new(value: i64) -> Self {
        Self { value }
    }

    pub fn zero() -> Self {
        Self { value: 0 }
    }

    pub fn from_major_units(major: i64, minor_units_per_major: i64) -> Self {
        Self {
            value: major * minor_units_per_major,
        }
    }

    pub fn add(self, other: Amount) -> Result<Amount, crate::errors::LedgerError> {
        self.value
            .checked_add(other.value)
            .map(|value| Amount { value })
            .ok_or(crate::errors::LedgerError::AmountOverflow)
    }

    pub fn subtract(self, other: Amount) -> Result<Amount, crate::errors::LedgerError> {
        self.value
            .checked_sub(other.value)
            .map(|value| Amount { value })
            .ok_or(crate::errors::LedgerError::AmountUnderflow)
    }

    pub fn is_positive(self) -> bool {
        self.value > 0
    }

    pub fn is_negative(self) -> bool {
        self.value < 0
    }

    pub fn is_zero(self) -> bool {
        self.value == 0
    }
}

/// Currency code (ISO 4217)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Currency(pub String);

impl Currency {
    pub fn new(code: impl Into<String>) -> Self {
        Self(code.into().to_uppercase())
    }

    pub fn usd() -> Self {
        Self("USD".to_string())
    }

    pub fn eur() -> Self {
        Self("EUR".to_string())
    }
}

/// Account status
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountStatus {
    Active,
    Suspended,
    Closed,
}

/// Account type for regulatory compliance
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AccountType {
    Customer,
    Operational,
    Safeguarding,
    Nostro,
    Vostro,
}

/// Account metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub account_type: AccountType,
    pub currency: Currency,
    pub tags: HashMap<String, String>,
    pub regulatory_flags: Vec<String>,
}

/// Account aggregate
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Account {
    pub id: AccountId,
    pub status: AccountStatus,
    pub metadata: AccountMetadata,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Transaction type
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransactionType {
    Debit,
    Credit,
    Transfer,
}

/// Transaction metadata
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransactionMetadata {
    pub reference: Option<String>,
    pub description: Option<String>,
    pub tags: HashMap<String, String>,
    pub regulatory_flags: Vec<String>,
}

/// Balance snapshot for efficient queries
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Balance {
    pub account_id: AccountId,
    pub amount: Amount,
    pub currency: Currency,
    pub as_of: DateTime<Utc>,
    pub version: u64,
}

/// Merkle tree node for cryptographic verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleNode {
    pub hash: String,
    pub left_child: Option<Box<MerkleNode>>,
    pub right_child: Option<Box<MerkleNode>>,
}

/// Merkle inclusion proof
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MerkleProof {
    pub leaf_hash: String,
    pub root_hash: String,
    pub proof_path: Vec<String>,
    pub leaf_index: usize,
}

/// Ledger digest for external verification
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LedgerDigest {
    pub root_hash: String,
    pub block_height: u64,
    pub timestamp: DateTime<Utc>,
    pub transaction_count: u64,
}