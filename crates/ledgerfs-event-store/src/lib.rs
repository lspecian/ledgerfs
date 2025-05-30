//! LedgerFS Event Store
//! 
//! This crate provides the core Event Store implementation using RocksDB as the LSM tree backend.
//! It implements an append-only, immutable storage for all ledger events with high-throughput
//! write operations and efficient read access patterns.

pub mod store;
pub mod serialization;
pub mod batch;
pub mod recovery;
pub mod config;

#[cfg(test)]
pub mod serialization_tests;

pub use store::*;
pub use serialization::*;
pub use batch::*;
pub use recovery::*;
pub use config::*;

// Re-export core types
pub use ledgerfs_core::*;