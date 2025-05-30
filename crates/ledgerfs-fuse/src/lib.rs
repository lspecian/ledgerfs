pub mod filesystem;
pub mod mount;
pub mod path_resolver;

pub use filesystem::*;
pub use mount::*;
pub use path_resolver::*;

use ledgerfs_core::*;
use std::sync::Arc;

/// LedgerFS FUSE configuration
#[derive(Debug, Clone)]
pub struct FuseConfig {
    pub mount_point: String,
    pub read_only: bool,
    pub allow_other: bool,
    pub auto_unmount: bool,
}

impl Default for FuseConfig {
    fn default() -> Self {
        Self {
            mount_point: "/tmp/ledgerfs".to_string(),
            read_only: true,
            allow_other: false,
            auto_unmount: true,
        }
    }
}

/// Main FUSE service for LedgerFS
pub struct LedgerFuseService {
    config: FuseConfig,
    filesystem: Arc<LedgerFilesystem>,
}

impl LedgerFuseService {
    pub fn new(
        config: FuseConfig,
        balance_repo: Arc<dyn BalanceRepository>,
        account_repo: Arc<dyn AccountRepository>,
        query_service: Arc<dyn QueryService>,
    ) -> Self {
        let filesystem = Arc::new(LedgerFilesystem::new(
            balance_repo,
            account_repo,
            query_service,
        ));

        Self { config, filesystem }
    }

    /// Mount the filesystem
    pub async fn mount(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Mounting LedgerFS at {}", self.config.mount_point);

        let options = vec![
            fuser::MountOption::RO,
            fuser::MountOption::FSName("ledgerfs".to_string()),
        ];

        let filesystem = self.filesystem.clone();
        let mount_point = self.config.mount_point.clone();
        
        tokio::task::spawn_blocking(move || {
            // Try to unwrap the Arc, or clone the inner filesystem
            let fs = Arc::try_unwrap(filesystem).unwrap_or_else(|arc| (*arc).clone());
            fuser::mount2(fs, &mount_point, &options)
        }).await??;

        Ok(())
    }

    /// Unmount the filesystem
    pub fn unmount(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Unmounting LedgerFS from {}", self.config.mount_point);
        
        #[cfg(target_os = "linux")]
        {
            use std::process::Command;
            let output = Command::new("fusermount")
                .arg("-u")
                .arg(&self.config.mount_point)
                .output()?;
            
            if !output.status.success() {
                return Err(format!(
                    "Failed to unmount: {}",
                    String::from_utf8_lossy(&output.stderr)
                ).into());
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            let output = Command::new("umount")
                .arg(&self.config.mount_point)
                .output()?;
            
            if !output.status.success() {
                return Err(format!(
                    "Failed to unmount: {}",
                    String::from_utf8_lossy(&output.stderr)
                ).into());
            }
        }

        Ok(())
    }
}