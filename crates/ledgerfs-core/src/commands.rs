use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::types::*;

/// Base command trait for all ledger commands
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandEnvelope {
    pub command_id: String,
    pub command_type: String,
    pub aggregate_id: String,
    pub expected_version: Option<u64>,
    pub timestamp: DateTime<Utc>,
    pub command_data: LedgerCommand,
    pub metadata: CommandMetadata,
}

/// Metadata for commands
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandMetadata {
    pub correlation_id: Option<String>,
    pub user_id: Option<String>,
    pub source: String,
    pub idempotency_key: Option<String>,
}

/// All possible ledger commands
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum LedgerCommand {
    CreateAccount(CreateAccountCommand),
    ChangeAccountStatus(ChangeAccountStatusCommand),
    UpdateAccountMetadata(UpdateAccountMetadataCommand),
    ProcessTransaction(ProcessTransactionCommand),
    GenerateMerkleRoot(GenerateMerkleRootCommand),
    PublishDigest(PublishDigestCommand),
}

/// Create account command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAccountCommand {
    pub account_id: AccountId,
    pub account_type: AccountType,
    pub currency: Currency,
    pub initial_balance: Amount,
    pub metadata: AccountMetadata,
}

/// Change account status command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeAccountStatusCommand {
    pub account_id: AccountId,
    pub new_status: AccountStatus,
    pub reason: Option<String>,
}

/// Update account metadata command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UpdateAccountMetadataCommand {
    pub account_id: AccountId,
    pub new_metadata: AccountMetadata,
}

/// Process transaction command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessTransactionCommand {
    pub transaction_id: TransactionId,
    pub transaction_type: TransactionType,
    pub from_account: Option<AccountId>,
    pub to_account: Option<AccountId>,
    pub amount: Amount,
    pub currency: Currency,
    pub metadata: TransactionMetadata,
}

/// Generate Merkle root command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GenerateMerkleRootCommand {
    pub block_height: u64,
    pub transaction_ids: Vec<TransactionId>,
}

/// Publish digest command
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishDigestCommand {
    pub digest: LedgerDigest,
    pub storage_location: String,
}

impl CommandEnvelope {
    pub fn new(
        aggregate_id: String,
        expected_version: Option<u64>,
        command_data: LedgerCommand,
        metadata: CommandMetadata,
    ) -> Self {
        let command_type = match &command_data {
            LedgerCommand::CreateAccount(_) => "CreateAccount",
            LedgerCommand::ChangeAccountStatus(_) => "ChangeAccountStatus",
            LedgerCommand::UpdateAccountMetadata(_) => "UpdateAccountMetadata",
            LedgerCommand::ProcessTransaction(_) => "ProcessTransaction",
            LedgerCommand::GenerateMerkleRoot(_) => "GenerateMerkleRoot",
            LedgerCommand::PublishDigest(_) => "PublishDigest",
        };

        Self {
            command_id: uuid::Uuid::new_v4().to_string(),
            command_type: command_type.to_string(),
            aggregate_id,
            expected_version,
            timestamp: Utc::now(),
            command_data,
            metadata,
        }
    }
}

impl Default for CommandMetadata {
    fn default() -> Self {
        Self {
            correlation_id: None,
            user_id: None,
            source: "ledgerfs".to_string(),
            idempotency_key: None,
        }
    }
}

/// Command validation result
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandValidationResult {
    Valid,
    Invalid(Vec<String>),
}

impl CommandValidationResult {
    pub fn is_valid(&self) -> bool {
        matches!(self, CommandValidationResult::Valid)
    }

    pub fn errors(&self) -> Vec<String> {
        match self {
            CommandValidationResult::Valid => vec![],
            CommandValidationResult::Invalid(errors) => errors.clone(),
        }
    }
}