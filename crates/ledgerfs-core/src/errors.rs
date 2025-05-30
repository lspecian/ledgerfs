use thiserror::Error;
use crate::types::*;

/// Main error type for LedgerFS operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    #[error("Account not found: {account_id:?}")]
    AccountNotFound { account_id: AccountId },

    #[error("Transaction not found: {transaction_id:?}")]
    TransactionNotFound { transaction_id: TransactionId },

    #[error("Insufficient funds: account {account_id:?}, available: {available:?}, required: {required:?}")]
    InsufficientFunds {
        account_id: AccountId,
        available: Amount,
        required: Amount,
    },

    #[error("Account is not active: {account_id:?}, status: {status:?}")]
    AccountNotActive {
        account_id: AccountId,
        status: AccountStatus,
    },

    #[error("Currency mismatch: expected {expected:?}, got {actual:?}")]
    CurrencyMismatch { expected: Currency, actual: Currency },

    #[error("Amount overflow")]
    AmountOverflow,

    #[error("Amount underflow")]
    AmountUnderflow,

    #[error("Invalid amount: {amount:?}")]
    InvalidAmount { amount: Amount },

    #[error("Duplicate transaction: {transaction_id:?}")]
    DuplicateTransaction { transaction_id: TransactionId },

    #[error("Duplicate account: {account_id:?}")]
    DuplicateAccount { account_id: AccountId },

    #[error("Concurrency conflict: expected version {expected}, actual version {actual}")]
    ConcurrencyConflict { expected: u64, actual: u64 },

    #[error("Command validation failed: {errors:?}")]
    CommandValidationFailed { errors: Vec<String> },

    #[error("Event store error: {message}")]
    EventStoreError { message: String },

    #[error("Serialization error: {message}")]
    SerializationError { message: String },

    #[error("Cryptographic error: {message}")]
    CryptographicError { message: String },

    #[error("Merkle proof verification failed")]
    MerkleProofVerificationFailed,

    #[error("Digest verification failed")]
    DigestVerificationFailed,

    #[error("Storage error: {message}")]
    StorageError { message: String },

    #[error("Network error: {message}")]
    NetworkError { message: String },

    #[error("Configuration error: {message}")]
    ConfigurationError { message: String },

    #[error("Permission denied: {operation}")]
    PermissionDenied { operation: String },

    #[error("Rate limit exceeded")]
    RateLimitExceeded,

    #[error("Service unavailable: {service}")]
    ServiceUnavailable { service: String },

    #[error("Timeout: {operation}")]
    Timeout { operation: String },

    #[error("Internal error: {message}")]
    Internal { message: String },
}

impl LedgerError {
    pub fn event_store(message: impl Into<String>) -> Self {
        Self::EventStoreError {
            message: message.into(),
        }
    }

    pub fn serialization(message: impl Into<String>) -> Self {
        Self::SerializationError {
            message: message.into(),
        }
    }

    pub fn cryptographic(message: impl Into<String>) -> Self {
        Self::CryptographicError {
            message: message.into(),
        }
    }

    pub fn storage(message: impl Into<String>) -> Self {
        Self::StorageError {
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }

    pub fn configuration(message: impl Into<String>) -> Self {
        Self::ConfigurationError {
            message: message.into(),
        }
    }

    pub fn permission_denied(operation: impl Into<String>) -> Self {
        Self::PermissionDenied {
            operation: operation.into(),
        }
    }

    pub fn service_unavailable(service: impl Into<String>) -> Self {
        Self::ServiceUnavailable {
            service: service.into(),
        }
    }

    pub fn timeout(operation: impl Into<String>) -> Self {
        Self::Timeout {
            operation: operation.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::Internal {
            message: message.into(),
        }
    }
}

/// Result type for LedgerFS operations
pub type LedgerResult<T> = Result<T, LedgerError>;

/// Error type for FUSE operations
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum FuseError {
    #[error("File not found: {path}")]
    FileNotFound { path: String },

    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },

    #[error("Invalid path: {path}")]
    InvalidPath { path: String },

    #[error("IO error: {message}")]
    IoError { message: String },

    #[error("Ledger error: {error:?}")]
    LedgerError { error: LedgerError },
}

impl From<LedgerError> for FuseError {
    fn from(error: LedgerError) -> Self {
        Self::LedgerError { error }
    }
}

/// Result type for FUSE operations
pub type FuseResult<T> = Result<T, FuseError>;