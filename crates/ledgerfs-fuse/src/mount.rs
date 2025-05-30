use crate::{FuseConfig, LedgerFilesystem};
use ledgerfs_core::*;
use std::sync::Arc;
use tokio::signal;

/// Mount manager for LedgerFS FUSE
pub struct MountManager {
    config: FuseConfig,
    filesystem: Arc<LedgerFilesystem>,
}

impl MountManager {
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

    /// Mount the filesystem and wait for shutdown signal
    pub async fn mount_and_serve(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::info!("Mounting LedgerFS at {}", self.config.mount_point);

        // Ensure mount point exists
        std::fs::create_dir_all(&self.config.mount_point)?;

        let options = vec![
            fuser::MountOption::RO,
            fuser::MountOption::FSName("ledgerfs".to_string()),
        ];

        let mount_point = self.config.mount_point.clone();
        let filesystem = self.filesystem.clone();
        
        // Spawn the FUSE mount in a blocking task
        let mount_handle = tokio::task::spawn_blocking(move || {
            // Try to unwrap the Arc, or clone the inner filesystem
            let fs = Arc::try_unwrap(filesystem).unwrap_or_else(|arc| (*arc).clone());
            fuser::mount2(fs, &mount_point, &options)
        });

        tracing::info!("LedgerFS mounted successfully. Press Ctrl+C to unmount.");

        // Wait for shutdown signal
        tokio::select! {
            result = mount_handle => {
                match result {
                    Ok(Ok(())) => tracing::info!("FUSE mount completed normally"),
                    Ok(Err(e)) => tracing::error!("FUSE mount error: {}", e),
                    Err(e) => tracing::error!("Mount task error: {}", e),
                }
            }
            _ = signal::ctrl_c() => {
                tracing::info!("Received shutdown signal, unmounting...");
                self.unmount()?;
            }
        }

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
                tracing::warn!(
                    "fusermount failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                // Try lazy unmount as fallback
                let _ = Command::new("fusermount")
                    .arg("-uz")
                    .arg(&self.config.mount_point)
                    .output();
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            let output = Command::new("umount")
                .arg(&self.config.mount_point)
                .output()?;
            
            if !output.status.success() {
                tracing::warn!(
                    "umount failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                // Try force unmount as fallback
                let _ = Command::new("umount")
                    .arg("-f")
                    .arg(&self.config.mount_point)
                    .output();
            }
        }

        Ok(())
    }
}