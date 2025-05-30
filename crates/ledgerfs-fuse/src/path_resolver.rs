use ledgerfs_core::*;

/// Path resolver for FUSE filesystem
pub struct PathResolver;

impl PathResolver {
    /// Parse account ID from path
    pub fn parse_account_id(path: &str) -> Result<AccountId, FuseError> {
        let parts: Vec<&str> = path.split('/').collect();
        
        if parts.len() < 3 || parts[1] != "accounts" {
            return Err(FuseError::InvalidPath { path: path.to_string() });
        }
        
        let account_id_str = parts[2];
        let uuid = uuid::Uuid::parse_str(account_id_str)
            .map_err(|_| FuseError::InvalidPath { path: path.to_string() })?;
        
        Ok(AccountId(uuid))
    }

    /// Parse transaction ID from path
    pub fn parse_transaction_id(path: &str) -> Result<TransactionId, FuseError> {
        let parts: Vec<&str> = path.split('/').collect();
        
        if parts.len() < 3 || parts[1] != "transactions" {
            return Err(FuseError::InvalidPath { path: path.to_string() });
        }
        
        let tx_filename = parts[2];
        let tx_id_str = tx_filename.strip_suffix(".json")
            .ok_or_else(|| FuseError::InvalidPath { path: path.to_string() })?;
        
        let uuid = uuid::Uuid::parse_str(tx_id_str)
            .map_err(|_| FuseError::InvalidPath { path: path.to_string() })?;
        
        Ok(TransactionId(uuid))
    }

    /// Check if path is a directory
    pub fn is_directory(path: &str) -> bool {
        match path {
            "/" | "/accounts" | "/transactions" | "/docs" => true,
            path if path.starts_with("/accounts/") && path.matches('/').count() == 2 => true,
            path if path.starts_with("/docs/") && path.matches('/').count() == 2 => true,
            _ => false,
        }
    }

    /// Check if path is a valid file
    pub fn is_valid_file(path: &str) -> bool {
        match path {
            path if path.ends_with("/balance.json") => true,
            path if path.ends_with("/tx.log") => true,
            path if path.ends_with("/snapshot.json") => true,
            path if path.starts_with("/transactions/") && path.ends_with(".json") => true,
            path if path.starts_with("/docs/") && (path.ends_with(".json") || path.ends_with(".pdf")) => true,
            _ => false,
        }
    }

    /// Get file type from path
    pub fn get_file_type(path: &str) -> FileType {
        if Self::is_directory(path) {
            FileType::Directory
        } else if path.ends_with(".json") {
            FileType::Json
        } else if path.ends_with(".log") {
            FileType::Log
        } else if path.ends_with(".pdf") {
            FileType::Pdf
        } else {
            FileType::Unknown
        }
    }

    /// Generate account directory path
    pub fn account_path(account_id: &AccountId) -> String {
        format!("/accounts/{}", account_id.0)
    }

    /// Generate balance file path
    pub fn balance_path(account_id: &AccountId) -> String {
        format!("/accounts/{}/balance.json", account_id.0)
    }

    /// Generate transaction log path
    pub fn transaction_log_path(account_id: &AccountId) -> String {
        format!("/accounts/{}/tx.log", account_id.0)
    }

    /// Generate snapshot path
    pub fn snapshot_path(account_id: &AccountId, timestamp: &str) -> String {
        format!("/accounts/{}/snapshot_{}.json", account_id.0, timestamp)
    }

    /// Generate transaction file path
    pub fn transaction_path(transaction_id: &TransactionId) -> String {
        format!("/transactions/{}.json", transaction_id.0)
    }

    /// Generate document path
    pub fn document_path(entity_type: &str, entity_id: &str, doc_name: &str) -> String {
        format!("/docs/{}/{}/{}", entity_type, entity_id, doc_name)
    }
}

/// File type enumeration
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileType {
    Directory,
    Json,
    Log,
    Pdf,
    Unknown,
}

impl FileType {
    pub fn mime_type(&self) -> &'static str {
        match self {
            FileType::Json => "application/json",
            FileType::Log => "text/plain",
            FileType::Pdf => "application/pdf",
            FileType::Directory => "inode/directory",
            FileType::Unknown => "application/octet-stream",
        }
    }

    pub fn is_text(&self) -> bool {
        matches!(self, FileType::Json | FileType::Log)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_account_id() {
        let account_id = AccountId::new();
        let path = format!("/accounts/{}/balance.json", account_id.0);
        
        let parsed = PathResolver::parse_account_id(&path).unwrap();
        assert_eq!(parsed, account_id);
    }

    #[test]
    fn test_is_directory() {
        assert!(PathResolver::is_directory("/"));
        assert!(PathResolver::is_directory("/accounts"));
        assert!(PathResolver::is_directory("/accounts/123e4567-e89b-12d3-a456-426614174000"));
        assert!(!PathResolver::is_directory("/accounts/123e4567-e89b-12d3-a456-426614174000/balance.json"));
    }

    #[test]
    fn test_file_type() {
        assert_eq!(PathResolver::get_file_type("/accounts/test/balance.json"), FileType::Json);
        assert_eq!(PathResolver::get_file_type("/accounts/test/tx.log"), FileType::Log);
        assert_eq!(PathResolver::get_file_type("/docs/test/doc.pdf"), FileType::Pdf);
    }
}