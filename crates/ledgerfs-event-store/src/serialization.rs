//! Event serialization and deserialization for the LedgerFS event store.
//! 
//! This module provides efficient serialization using bincode with optional
//! LZ4 compression for high-performance storage and retrieval of events.

use crate::config::EventStoreConfig;
use ledgerfs_core::{EventEnvelope, LedgerEvent, EventMetadata};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Errors that can occur during serialization/deserialization
#[derive(Debug, thiserror::Error)]
pub enum SerializationError {
    #[error("Serialization failed: {0}")]
    SerializationFailed(String),
    
    #[error("Compression failed: {0}")]
    CompressionFailed(String),
    
    #[error("Decompression failed: {0}")]
    DecompressionFailed(String),
    
    #[error("Invalid format version: expected {expected}, got {actual}")]
    InvalidFormatVersion { expected: u8, actual: u8 },
}

/// Serialized event envelope with metadata for storage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializedEventEnvelope {
    /// Format version for backward compatibility
    pub format_version: u8,
    
    /// Whether the data is compressed
    pub compressed: bool,
    
    /// The serialized event envelope data
    pub data: Vec<u8>,
}

/// Event serializer with configurable compression
pub struct EventSerializer {
    use_compression: bool,
}

impl EventSerializer {
    /// Create a new event serializer
    pub fn new(use_compression: bool) -> Self {
        Self { use_compression }
    }
    
    /// Create serializer from config
    pub fn from_config(config: &EventStoreConfig) -> Self {
        Self::new(config.enable_compression)
    }
    
    /// Serialize an event envelope
    pub fn serialize(&self, envelope: &EventEnvelope) -> Result<Vec<u8>, SerializationError> {
        // Directly serialize the envelope using bincode
        let envelope_bytes = bincode::serialize(envelope)
            .map_err(|e| SerializationError::SerializationFailed(e.to_string()))?;
        
        if self.use_compression {
            self.compress(&envelope_bytes)
        } else {
            Ok(envelope_bytes)
        }
    }
    
    /// Deserialize an event envelope
    pub fn deserialize(&self, data: &[u8]) -> Result<EventEnvelope, SerializationError> {
        let envelope_bytes = if self.use_compression {
            self.decompress(data)?
        } else {
            data.to_vec()
        };
        
        bincode::deserialize(&envelope_bytes)
            .map_err(|e| SerializationError::SerializationFailed(e.to_string()))
    }
    
    /// Compress data using LZ4
    fn compress(&self, data: &[u8]) -> Result<Vec<u8>, SerializationError> {
        let mut encoder = lz4_flex::frame::FrameEncoder::new(Vec::new());
        encoder.write_all(data)
            .map_err(|e| SerializationError::CompressionFailed(e.to_string()))?;
        encoder.finish()
            .map_err(|e| SerializationError::CompressionFailed(e.to_string()))
    }
    
    /// Decompress data using LZ4
    fn decompress(&self, data: &[u8]) -> Result<Vec<u8>, SerializationError> {
        let mut decoder = lz4_flex::frame::FrameDecoder::new(data);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed)
            .map_err(|e| SerializationError::DecompressionFailed(e.to_string()))?;
        Ok(decompressed)
    }
}

/// Generate a storage key for an event
/// 
/// Key format: timestamp (8 bytes, big-endian) + event_id (16 bytes)
/// This ensures events are naturally ordered by time while maintaining uniqueness
pub fn generate_event_key(envelope: &EventEnvelope) -> Vec<u8> {
    let mut key = Vec::with_capacity(24);
    
    // Add timestamp as big-endian u64 for natural ordering
    let timestamp_nanos = envelope.timestamp.timestamp_nanos_opt().unwrap_or(0) as u64;
    key.extend_from_slice(&timestamp_nanos.to_be_bytes());
    
    // Add event ID bytes for uniqueness
    key.extend_from_slice(&envelope.event_id.0.as_bytes()[..]);
    
    key
}

/// Generate a key prefix for querying events by account
pub fn generate_account_key_prefix(account_id: &str) -> Vec<u8> {
    format!("account:{}", account_id).into_bytes()
}

/// Generate a key prefix for querying events by time range
pub fn generate_time_key_prefix(timestamp: chrono::DateTime<chrono::Utc>) -> Vec<u8> {
    let timestamp_nanos = timestamp.timestamp_nanos_opt().unwrap_or(0) as u64;
    timestamp_nanos.to_be_bytes().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledgerfs_core::*;
    use std::collections::HashMap;

    #[test]
    fn test_serialization_roundtrip() {
        let serializer = EventSerializer::new(false);
        
        let account_id = AccountId::new();
        let event = AccountCreatedEvent {
            account_id: account_id.clone(),
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            initial_balance: Amount::new(1000),
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                tags: HashMap::new(),
                regulatory_flags: vec!["PSD2_COMPLIANT".to_string()],
            },
            created_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        assert_eq!(envelope.event_id, deserialized.event_id);
        assert_eq!(envelope.aggregate_id, deserialized.aggregate_id);
        assert_eq!(envelope.aggregate_version, deserialized.aggregate_version);
    }

    #[test]
    fn test_compression_serialization() {
        let serializer = EventSerializer::new(true);
        
        let account_id = AccountId::new();
        let mut large_tags = HashMap::new();
        for i in 0..100 {
            large_tags.insert(format!("key_{}", i), format!("value_{}", i));
        }
        
        let event = AccountCreatedEvent {
            account_id: account_id.clone(),
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            initial_balance: Amount::new(1000),
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                tags: large_tags,
                regulatory_flags: vec!["PSD2_COMPLIANT".to_string()],
            },
            created_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        assert_eq!(envelope.event_id, deserialized.event_id);
    }

    #[test]
    fn test_key_generation() {
        let account_id = AccountId::new();
        let event = AccountCreatedEvent {
            account_id: account_id.clone(),
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            initial_balance: Amount::new(1000),
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                tags: HashMap::new(),
                regulatory_flags: vec![],
            },
            created_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        let key = generate_event_key(&envelope);
        
        // Key should be 24 bytes (8 timestamp + 16 UUID)
        assert_eq!(key.len(), 24);
        
        // First 8 bytes should be timestamp
        let timestamp_bytes = &key[0..8];
        let timestamp = u64::from_be_bytes(timestamp_bytes.try_into().unwrap());
        assert!(timestamp > 0);
        
        // Last 16 bytes should be UUID
        let uuid_bytes = &key[8..24];
        assert_eq!(uuid_bytes.len(), 16);
    }
}