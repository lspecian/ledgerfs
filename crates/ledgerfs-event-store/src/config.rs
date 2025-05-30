//! Event Store Configuration

use std::path::PathBuf;

/// Configuration for the Event Store
#[derive(Debug, Clone)]
pub struct EventStoreConfig {
    /// Path to the RocksDB database directory
    pub db_path: PathBuf,
    
    /// Maximum number of events to batch in a single write operation
    pub batch_size: usize,
    
    /// Enable compression for storage efficiency
    pub enable_compression: bool,
    
    /// Compression algorithm to use
    pub compression_type: CompressionType,
    
    /// Maximum number of background threads for compaction
    pub max_background_jobs: i32,
    
    /// Write buffer size in bytes
    pub write_buffer_size: usize,
    
    /// Maximum number of write buffers
    pub max_write_buffer_number: i32,
    
    /// Target file size for level 0
    pub target_file_size_base: u64,
    
    /// Enable WAL (Write-Ahead Log) for durability
    pub enable_wal: bool,
    
    /// Sync writes to disk for durability
    pub sync_writes: bool,
}

/// Compression algorithms supported by RocksDB
#[derive(Debug, Clone, Copy)]
pub enum CompressionType {
    None,
    Snappy,
    Zlib,
    Bz2,
    Lz4,
    Lz4hc,
    Zstd,
}

impl Default for EventStoreConfig {
    fn default() -> Self {
        Self {
            db_path: PathBuf::from("./data/eventstore"),
            batch_size: 1000,
            enable_compression: true,
            compression_type: CompressionType::Lz4,
            max_background_jobs: 4,
            write_buffer_size: 64 * 1024 * 1024, // 64MB
            max_write_buffer_number: 3,
            target_file_size_base: 64 * 1024 * 1024, // 64MB
            enable_wal: true,
            sync_writes: false, // For performance, can be enabled for stronger durability
        }
    }
}

impl EventStoreConfig {
    /// Create a new configuration with the specified database path
    pub fn new(db_path: impl Into<PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
            ..Default::default()
        }
    }
    
    /// Configure for high-throughput scenarios
    pub fn high_throughput(mut self) -> Self {
        self.batch_size = 5000;
        self.write_buffer_size = 128 * 1024 * 1024; // 128MB
        self.max_write_buffer_number = 6;
        self.sync_writes = false;
        self.max_background_jobs = 8;
        self
    }
    
    /// Configure for high-durability scenarios
    pub fn high_durability(mut self) -> Self {
        self.sync_writes = true;
        self.enable_wal = true;
        self.batch_size = 100; // Smaller batches for faster commits
        self
    }
    
    /// Configure for development/testing
    pub fn development(mut self) -> Self {
        self.db_path = PathBuf::from("./tmp/eventstore-dev");
        self.sync_writes = false;
        self.enable_compression = false; // Faster for development
        self
    }
}

impl From<CompressionType> for rocksdb::DBCompressionType {
    fn from(compression: CompressionType) -> Self {
        match compression {
            CompressionType::None => rocksdb::DBCompressionType::None,
            CompressionType::Snappy => rocksdb::DBCompressionType::Snappy,
            CompressionType::Zlib => rocksdb::DBCompressionType::Zlib,
            CompressionType::Bz2 => rocksdb::DBCompressionType::Bz2,
            CompressionType::Lz4 => rocksdb::DBCompressionType::Lz4,
            CompressionType::Lz4hc => rocksdb::DBCompressionType::Lz4hc,
            CompressionType::Zstd => rocksdb::DBCompressionType::Zstd,
        }
    }
}