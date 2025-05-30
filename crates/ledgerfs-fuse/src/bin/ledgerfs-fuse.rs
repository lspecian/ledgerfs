use ledgerfs_core::*;
use ledgerfs_fuse::*;
use std::collections::HashMap;
use std::sync::Arc;
use tokio;
use tracing_subscriber;

/// Mock balance repository for testing
#[derive(Debug)]
struct MockBalanceRepository {
    balances: HashMap<AccountId, Balance>,
}

impl MockBalanceRepository {
    fn new(accounts: &[AccountId]) -> Self {
        let mut balances = HashMap::new();
        
        if accounts.len() >= 2 {
            balances.insert(
                accounts[0].clone(),
                Balance {
                    account_id: accounts[0].clone(),
                    amount: Amount::new(100000), // $1000.00
                    currency: Currency::usd(),
                    as_of: chrono::Utc::now(),
                    version: 1,
                },
            );
            
            balances.insert(
                accounts[1].clone(),
                Balance {
                    account_id: accounts[1].clone(),
                    amount: Amount::new(250000), // $2500.00
                    currency: Currency::eur(),
                    as_of: chrono::Utc::now(),
                    version: 1,
                },
            );
        }
        
        Self { balances }
    }
}

#[async_trait::async_trait]
impl BalanceRepository for MockBalanceRepository {
    async fn get_balance(&self, account_id: &AccountId) -> LedgerResult<Option<Balance>> {
        Ok(self.balances.get(account_id).cloned())
    }

    async fn update_balance(&self, _balance: Balance) -> LedgerResult<()> {
        Ok(())
    }

    async fn get_balances(&self, account_ids: &[AccountId]) -> LedgerResult<Vec<Balance>> {
        Ok(account_ids
            .iter()
            .filter_map(|id| self.balances.get(id).cloned())
            .collect())
    }
}

/// Mock account repository for testing
#[derive(Debug)]
struct MockAccountRepository {
    accounts: HashMap<AccountId, Account>,
}

impl MockAccountRepository {
    fn new(account_ids: &[AccountId]) -> Self {
        let mut accounts = HashMap::new();
        
        if account_ids.len() >= 2 {
            let mut tags1 = HashMap::new();
            tags1.insert("customer_id".to_string(), "cust_001".to_string());
            
            let mut tags2 = HashMap::new();
            tags2.insert("customer_id".to_string(), "cust_002".to_string());
            
            accounts.insert(
                account_ids[0].clone(),
                Account {
                    id: account_ids[0].clone(),
                    status: AccountStatus::Active,
                    metadata: AccountMetadata {
                        account_type: AccountType::Customer,
                        currency: Currency::usd(),
                        tags: tags1,
                        regulatory_flags: vec!["KYC_VERIFIED".to_string()],
                    },
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                },
            );
            
            accounts.insert(
                account_ids[1].clone(),
                Account {
                    id: account_ids[1].clone(),
                    status: AccountStatus::Active,
                    metadata: AccountMetadata {
                        account_type: AccountType::Customer,
                        currency: Currency::eur(),
                        tags: tags2,
                        regulatory_flags: vec!["KYC_VERIFIED".to_string(), "PEP_CHECK".to_string()],
                    },
                    created_at: chrono::Utc::now(),
                    updated_at: chrono::Utc::now(),
                },
            );
        }
        
        Self { accounts }
    }
}

#[async_trait::async_trait]
impl AccountRepository for MockAccountRepository {
    async fn get_account(&self, account_id: &AccountId) -> LedgerResult<Option<Account>> {
        Ok(self.accounts.get(account_id).cloned())
    }

    async fn save_account(&self, _account: Account) -> LedgerResult<()> {
        Ok(())
    }

    async fn find_accounts(&self, _criteria: HashMap<String, String>) -> LedgerResult<Vec<Account>> {
        Ok(self.accounts.values().cloned().collect())
    }
}

/// Mock query service for testing
#[derive(Debug)]
struct MockQueryService;

#[async_trait::async_trait]
impl QueryService for MockQueryService {
    async fn get_balance_at_time(
        &self,
        _account_id: &AccountId,
        _timestamp: chrono::DateTime<chrono::Utc>,
    ) -> LedgerResult<Option<Balance>> {
        Ok(None)
    }

    async fn get_transaction_history(
        &self,
        account_id: &AccountId,
        _from: Option<chrono::DateTime<chrono::Utc>>,
        _to: Option<chrono::DateTime<chrono::Utc>>,
        limit: Option<usize>,
    ) -> LedgerResult<Vec<TransactionProcessedEvent>> {
        // Generate some mock transactions
        let mut transactions = Vec::new();
        let limit = limit.unwrap_or(10);
        
        for i in 0..limit.min(5) {
            transactions.push(TransactionProcessedEvent {
                transaction_id: TransactionId::new(),
                transaction_type: if i % 2 == 0 { TransactionType::Credit } else { TransactionType::Debit },
                from_account: if i % 2 == 0 { None } else { Some(account_id.clone()) },
                to_account: if i % 2 == 0 { Some(account_id.clone()) } else { None },
                amount: Amount::new((i as i64 + 1) * 1000),
                currency: Currency::usd(),
                metadata: TransactionMetadata {
                    reference: Some(format!("REF_{:03}", i)),
                    description: Some(format!("Test transaction {}", i)),
                    tags: HashMap::new(),
                    regulatory_flags: vec![],
                },
                processed_at: chrono::Utc::now() - chrono::Duration::hours(i as i64),
            });
        }
        
        Ok(transactions)
    }

    async fn search_transactions(
        &self,
        _criteria: HashMap<String, String>,
        _limit: Option<usize>,
    ) -> LedgerResult<Vec<TransactionProcessedEvent>> {
        Ok(vec![])
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt()
        .with_env_filter("ledgerfs_fuse=debug,info")
        .init();

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    let mount_point = args.get(1)
        .map(|s| s.clone())
        .unwrap_or_else(|| "/tmp/ledgerfs".to_string());

    println!("LedgerFS FUSE - Mounting at: {}", mount_point);
    println!("Usage: {} [mount_point]", args[0]);
    println!("Press Ctrl+C to unmount and exit");

    // Create consistent account IDs for all repositories
    let account_ids = vec![AccountId::new(), AccountId::new()];

    // Create mock repositories with consistent account IDs
    let balance_repo: Arc<dyn BalanceRepository> = Arc::new(MockBalanceRepository::new(&account_ids));
    let account_repo: Arc<dyn AccountRepository> = Arc::new(MockAccountRepository::new(&account_ids));
    let query_service: Arc<dyn QueryService> = Arc::new(MockQueryService);

    // Create FUSE configuration
    let config = FuseConfig {
        mount_point,
        read_only: true,
        allow_other: false,
        auto_unmount: true,
    };

    // Create and start the mount manager
    let mount_manager = MountManager::new(
        config,
        balance_repo,
        account_repo,
        query_service,
    );

    // Mount and serve
    mount_manager.mount_and_serve().await?;

    println!("LedgerFS unmounted successfully");
    Ok(())
}