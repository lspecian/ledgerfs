use fuser::{
    FileAttr, FileType, Filesystem, ReplyAttr, ReplyData, ReplyDirectory, ReplyEntry, Request,
};
use ledgerfs_core::*;
use libc::ENOENT;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::sync::Arc;
use std::time::{Duration, UNIX_EPOCH};
use parking_lot::RwLock;

const TTL: Duration = Duration::from_secs(1);

/// FUSE filesystem implementation for LedgerFS
pub struct LedgerFilesystem {
    balance_repo: Arc<dyn BalanceRepository>,
    account_repo: Arc<dyn AccountRepository>,
    query_service: Arc<dyn QueryService>,
    inode_map: RwLock<HashMap<u64, String>>,
    path_map: RwLock<HashMap<String, u64>>,
    next_inode: RwLock<u64>,
}

impl Clone for LedgerFilesystem {
    fn clone(&self) -> Self {
        Self {
            balance_repo: self.balance_repo.clone(),
            account_repo: self.account_repo.clone(),
            query_service: self.query_service.clone(),
            inode_map: RwLock::new(self.inode_map.read().clone()),
            path_map: RwLock::new(self.path_map.read().clone()),
            next_inode: RwLock::new(*self.next_inode.read()),
        }
    }
}

impl LedgerFilesystem {
    pub fn new(
        balance_repo: Arc<dyn BalanceRepository>,
        account_repo: Arc<dyn AccountRepository>,
        query_service: Arc<dyn QueryService>,
    ) -> Self {
        let mut inode_map = HashMap::new();
        let mut path_map = HashMap::new();
        
        // Root directory
        inode_map.insert(1, "/".to_string());
        path_map.insert("/".to_string(), 1);
        
        // Main directories
        inode_map.insert(2, "/accounts".to_string());
        path_map.insert("/accounts".to_string(), 2);
        
        inode_map.insert(3, "/transactions".to_string());
        path_map.insert("/transactions".to_string(), 3);
        
        inode_map.insert(4, "/docs".to_string());
        path_map.insert("/docs".to_string(), 4);

        Self {
            balance_repo,
            account_repo,
            query_service,
            inode_map: RwLock::new(inode_map),
            path_map: RwLock::new(path_map),
            next_inode: RwLock::new(5),
        }
    }

    fn get_or_create_inode(&self, path: &str) -> u64 {
        {
            let path_map = self.path_map.read();
            if let Some(&inode) = path_map.get(path) {
                return inode;
            }
        }

        let mut next_inode = self.next_inode.write();
        let mut inode_map = self.inode_map.write();
        let mut path_map = self.path_map.write();

        // Double-check after acquiring write lock
        if let Some(&inode) = path_map.get(path) {
            return inode;
        }

        let inode = *next_inode;
        *next_inode += 1;
        
        inode_map.insert(inode, path.to_string());
        path_map.insert(path.to_string(), inode);
        
        inode
    }

    fn get_path(&self, inode: u64) -> Option<String> {
        self.inode_map.read().get(&inode).cloned()
    }

    fn create_file_attr(&self, inode: u64, size: u64, is_dir: bool) -> FileAttr {
        let now = UNIX_EPOCH + Duration::from_secs(chrono::Utc::now().timestamp() as u64);
        
        FileAttr {
            ino: inode,
            size,
            blocks: (size + 511) / 512,
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
            kind: if is_dir { FileType::Directory } else { FileType::RegularFile },
            perm: if is_dir { 0o755 } else { 0o644 },
            nlink: 1,
            uid: 1000,
            gid: 1000,
            rdev: 0,
            flags: 0,
            blksize: 4096,
        }
    }

    async fn read_account_balance(&self, account_id: &str) -> Result<String, FuseError> {
        let account_id = AccountId(uuid::Uuid::parse_str(account_id)
            .map_err(|_| FuseError::InvalidPath { path: account_id.to_string() })?);
        
        let balance = self.balance_repo.get_balance(&account_id).await?
            .ok_or_else(|| FuseError::FileNotFound { path: format!("balance for {}", account_id.0) })?;
        
        let balance_data = serde_json::json!({
            "account_id": balance.account_id.0,
            "amount": balance.amount.value,
            "currency": balance.currency.0,
            "as_of": balance.as_of,
            "version": balance.version
        });
        
        Ok(serde_json::to_string_pretty(&balance_data).unwrap())
    }

    async fn read_account_transactions(&self, account_id: &str) -> Result<String, FuseError> {
        let account_id = AccountId(uuid::Uuid::parse_str(account_id)
            .map_err(|_| FuseError::InvalidPath { path: account_id.to_string() })?);
        
        let transactions = self.query_service.get_transaction_history(
            &account_id,
            None,
            None,
            Some(100), // Limit to last 100 transactions
        ).await?;
        
        let tx_data = serde_json::json!({
            "account_id": account_id.0,
            "transaction_count": transactions.len(),
            "transactions": transactions
        });
        
        Ok(serde_json::to_string_pretty(&tx_data).unwrap())
    }

    async fn list_accounts(&self) -> Result<Vec<String>, FuseError> {
        let accounts = self.account_repo.find_accounts(HashMap::new()).await?;
        Ok(accounts.into_iter().map(|acc| acc.id.0.to_string()).collect())
    }
}

impl Filesystem for LedgerFilesystem {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_string_lossy();
        let parent_path = match self.get_path(parent) {
            Some(path) => path,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let full_path = if parent_path == "/" {
            format!("/{}", name)
        } else {
            format!("{}/{}", parent_path, name)
        };

        let inode = self.get_or_create_inode(&full_path);
        
        // Determine if this is a directory or file based on path structure
        let is_dir = match full_path.as_str() {
            "/accounts" | "/transactions" | "/docs" => true,
            path if path.starts_with("/accounts/") && path.matches('/').count() == 2 => true,
            path if path.starts_with("/docs/") && path.matches('/').count() == 2 => true,
            _ => false,
        };

        let attr = self.create_file_attr(inode, 0, is_dir);
        reply.entry(&TTL, &attr, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, reply: ReplyAttr) {
        let path = match self.get_path(ino) {
            Some(path) => path,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let is_dir = match path.as_str() {
            "/" | "/accounts" | "/transactions" | "/docs" => true,
            path if path.starts_with("/accounts/") && path.matches('/').count() == 2 => true,
            path if path.starts_with("/docs/") && path.matches('/').count() == 2 => true,
            _ => false,
        };

        // Estimate file size for non-directories
        let size = if is_dir { 0 } else { 1024 }; // Default size for JSON files
        
        let attr = self.create_file_attr(ino, size, is_dir);
        reply.attr(&TTL, &attr);
    }

    fn read(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        size: u32,
        _flags: i32,
        _lock: Option<u64>,
        reply: ReplyData,
    ) {
        let path = match self.get_path(ino) {
            Some(path) => path,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let content = match tokio::runtime::Runtime::new()
            .expect("Failed to create runtime")
            .block_on(async {
            if path.ends_with("/balance.json") {
                let account_id = path.split('/').nth(2).unwrap();
                self.read_account_balance(account_id).await
            } else if path.ends_with("/tx.log") {
                let account_id = path.split('/').nth(2).unwrap();
                self.read_account_transactions(account_id).await
            } else {
                Err(FuseError::FileNotFound { path: path.clone() })
            }
        }) {
            Ok(content) => content,
            Err(_) => {
                reply.error(ENOENT);
                return;
            }
        };

        let content_bytes = content.as_bytes();
        let start = offset as usize;
        let end = std::cmp::min(start + size as usize, content_bytes.len());
        
        if start >= content_bytes.len() {
            reply.data(&[]);
        } else {
            reply.data(&content_bytes[start..end]);
        }
    }

    fn readdir(
        &mut self,
        _req: &Request,
        ino: u64,
        _fh: u64,
        offset: i64,
        mut reply: ReplyDirectory,
    ) {
        let path = match self.get_path(ino) {
            Some(path) => path,
            None => {
                reply.error(ENOENT);
                return;
            }
        };

        let entries = match path.as_str() {
            "/" => vec![
                (".", 1, FileType::Directory),
                ("..", 1, FileType::Directory),
                ("accounts", 2, FileType::Directory),
                ("transactions", 3, FileType::Directory),
                ("docs", 4, FileType::Directory),
            ],
            "/accounts" => {
                let mut entries = vec![
                    (".", 2, FileType::Directory),
                    ("..", 1, FileType::Directory),
                ];
                
                if let Ok(accounts) = tokio::runtime::Runtime::new()
                    .expect("Failed to create runtime")
                    .block_on(self.list_accounts()) {
                    for account_id in accounts {
                        let account_path = format!("/accounts/{}", account_id);
                        let inode = self.get_or_create_inode(&account_path);
                        entries.push((account_id.leak(), inode, FileType::Directory));
                    }
                }
                entries
            },
            path if path.starts_with("/accounts/") && path.matches('/').count() == 2 => {
                let account_inode = ino;
                let balance_path = format!("{}/balance.json", path);
                let tx_path = format!("{}/tx.log", path);
                
                let balance_inode = self.get_or_create_inode(&balance_path);
                let tx_inode = self.get_or_create_inode(&tx_path);
                
                vec![
                    (".", account_inode, FileType::Directory),
                    ("..", 2, FileType::Directory),
                    ("balance.json", balance_inode, FileType::RegularFile),
                    ("tx.log", tx_inode, FileType::RegularFile),
                ]
            },
            "/transactions" => vec![
                (".", 3, FileType::Directory),
                ("..", 1, FileType::Directory),
            ],
            "/docs" => vec![
                (".", 4, FileType::Directory),
                ("..", 1, FileType::Directory),
            ],
            _ => {
                reply.error(ENOENT);
                return;
            }
        };

        for (i, entry) in entries.into_iter().enumerate().skip(offset as usize) {
            if reply.add(entry.1, (i + 1) as i64, entry.2, entry.0) {
                break;
            }
        }
        reply.ok();
    }
}