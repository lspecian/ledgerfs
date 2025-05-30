//! Comprehensive tests for event schema and serialization
//! 
//! This module contains extensive tests to verify that all event types
//! can be correctly serialized and deserialized without data loss.

#[cfg(test)]
mod tests {
    use super::super::serialization::*;
    use ledgerfs_core::*;
    use std::collections::HashMap;

    /// Test all event types for serialization roundtrip
    #[test]
    fn test_all_event_types_serialization() {
        let serializer = EventSerializer::new(false);
        
        // Test AccountCreated event
        test_account_created_event(&serializer);
        
        // Test AccountStatusChanged event
        test_account_status_changed_event(&serializer);
        
        // Test AccountMetadataUpdated event
        test_account_metadata_updated_event(&serializer);
        
        // Test TransactionProcessed event
        test_transaction_processed_event(&serializer);
        
        // Test BalanceUpdated event
        test_balance_updated_event(&serializer);
        
        // Test MerkleRootGenerated event
        test_merkle_root_generated_event(&serializer);
        
        // Test DigestPublished event
        test_digest_published_event(&serializer);
    }

    fn test_account_created_event(serializer: &EventSerializer) {
        let mut tags = HashMap::new();
        tags.insert("customer_tier".to_string(), "premium".to_string());
        tags.insert("region".to_string(), "EU".to_string());
        
        let event = AccountCreatedEvent {
            account_id: AccountId::new(),
            account_type: AccountType::Customer,
            currency: Currency::eur(),
            initial_balance: Amount::new(50000), // €500.00
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::eur(),
                tags,
                regulatory_flags: vec!["PSD2_COMPLIANT".to_string(), "GDPR_COMPLIANT".to_string()],
            },
            created_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            event.account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event.clone()),
            EventMetadata {
                correlation_id: Some("test-correlation-123".to_string()),
                causation_id: Some("test-causation-456".to_string()),
                user_id: Some("user-789".to_string()),
                source: "ledgerfs-test".to_string(),
            },
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        assert_eq!(envelope.event_id, deserialized.event_id);
        assert_eq!(envelope.event_type, deserialized.event_type);
        assert_eq!(envelope.aggregate_id, deserialized.aggregate_id);
        assert_eq!(envelope.aggregate_version, deserialized.aggregate_version);
        
        if let LedgerEvent::AccountCreated(deserialized_event) = deserialized.event_data {
            assert_eq!(event.account_id, deserialized_event.account_id);
            assert_eq!(event.account_type, deserialized_event.account_type);
            assert_eq!(event.currency, deserialized_event.currency);
            assert_eq!(event.initial_balance, deserialized_event.initial_balance);
            assert_eq!(event.metadata.tags, deserialized_event.metadata.tags);
            assert_eq!(event.metadata.regulatory_flags, deserialized_event.metadata.regulatory_flags);
        } else {
            panic!("Expected AccountCreated event");
        }
    }

    fn test_account_status_changed_event(serializer: &EventSerializer) {
        let account_id = AccountId::new();
        let event = AccountStatusChangedEvent {
            account_id: account_id.clone(),
            old_status: AccountStatus::Active,
            new_status: AccountStatus::Suspended,
            reason: Some("Suspicious activity detected".to_string()),
            changed_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            2,
            LedgerEvent::AccountStatusChanged(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::AccountStatusChanged(deserialized_event) = deserialized.event_data {
            assert_eq!(event.account_id, deserialized_event.account_id);
            assert_eq!(event.old_status, deserialized_event.old_status);
            assert_eq!(event.new_status, deserialized_event.new_status);
            assert_eq!(event.reason, deserialized_event.reason);
        } else {
            panic!("Expected AccountStatusChanged event");
        }
    }

    fn test_account_metadata_updated_event(serializer: &EventSerializer) {
        let account_id = AccountId::new();
        
        let old_metadata = AccountMetadata {
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            tags: HashMap::new(),
            regulatory_flags: vec!["PSD2_COMPLIANT".to_string()],
        };
        
        let mut new_tags = HashMap::new();
        new_tags.insert("risk_level".to_string(), "low".to_string());
        
        let new_metadata = AccountMetadata {
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            tags: new_tags,
            regulatory_flags: vec!["PSD2_COMPLIANT".to_string(), "AML_VERIFIED".to_string()],
        };
        
        let event = AccountMetadataUpdatedEvent {
            account_id: account_id.clone(),
            old_metadata: old_metadata.clone(),
            new_metadata: new_metadata.clone(),
            updated_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            3,
            LedgerEvent::AccountMetadataUpdated(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::AccountMetadataUpdated(deserialized_event) = deserialized.event_data {
            assert_eq!(event.account_id, deserialized_event.account_id);
            assert_eq!(event.old_metadata.tags, deserialized_event.old_metadata.tags);
            assert_eq!(event.new_metadata.tags, deserialized_event.new_metadata.tags);
            assert_eq!(event.old_metadata.regulatory_flags, deserialized_event.old_metadata.regulatory_flags);
            assert_eq!(event.new_metadata.regulatory_flags, deserialized_event.new_metadata.regulatory_flags);
        } else {
            panic!("Expected AccountMetadataUpdated event");
        }
    }

    fn test_transaction_processed_event(serializer: &EventSerializer) {
        let from_account = AccountId::new();
        let to_account = AccountId::new();
        let transaction_id = TransactionId::new();
        
        let mut metadata_tags = HashMap::new();
        metadata_tags.insert("payment_method".to_string(), "SEPA".to_string());
        metadata_tags.insert("reference".to_string(), "INV-2024-001".to_string());
        
        let event = TransactionProcessedEvent {
            transaction_id: transaction_id.clone(),
            transaction_type: TransactionType::Transfer,
            from_account: Some(from_account.clone()),
            to_account: Some(to_account.clone()),
            amount: Amount::new(100000), // €1000.00
            currency: Currency::eur(),
            metadata: TransactionMetadata {
                description: Some("Invoice payment".to_string()),
                reference: Some("INV-2024-001".to_string()),
                tags: metadata_tags,
                regulatory_flags: vec!["AML_CHECKED".to_string()],
            },
            processed_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            transaction_id.0.to_string(),
            1,
            LedgerEvent::TransactionProcessed(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::TransactionProcessed(deserialized_event) = deserialized.event_data {
            assert_eq!(event.transaction_id, deserialized_event.transaction_id);
            assert_eq!(event.transaction_type, deserialized_event.transaction_type);
            assert_eq!(event.from_account, deserialized_event.from_account);
            assert_eq!(event.to_account, deserialized_event.to_account);
            assert_eq!(event.amount, deserialized_event.amount);
            assert_eq!(event.currency, deserialized_event.currency);
            assert_eq!(event.metadata.tags, deserialized_event.metadata.tags);
        } else {
            panic!("Expected TransactionProcessed event");
        }
    }

    fn test_balance_updated_event(serializer: &EventSerializer) {
        let account_id = AccountId::new();
        let transaction_id = TransactionId::new();
        
        let event = BalanceUpdatedEvent {
            account_id: account_id.clone(),
            old_balance: Amount::new(50000), // €500.00
            new_balance: Amount::new(150000), // €1500.00
            currency: Currency::eur(),
            transaction_id: Some(transaction_id.clone()),
            updated_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            account_id.0.to_string(),
            4,
            LedgerEvent::BalanceUpdated(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::BalanceUpdated(deserialized_event) = deserialized.event_data {
            assert_eq!(event.account_id, deserialized_event.account_id);
            assert_eq!(event.old_balance, deserialized_event.old_balance);
            assert_eq!(event.new_balance, deserialized_event.new_balance);
            assert_eq!(event.currency, deserialized_event.currency);
            assert_eq!(event.transaction_id, deserialized_event.transaction_id);
        } else {
            panic!("Expected BalanceUpdated event");
        }
    }

    fn test_merkle_root_generated_event(serializer: &EventSerializer) {
        let event = MerkleRootGeneratedEvent {
            block_height: 12345,
            root_hash: "a1b2c3d4e5f6789012345678901234567890abcdef1234567890abcdef123456".to_string(),
            transaction_count: 1000,
            previous_root_hash: Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string()),
            generated_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            format!("block-{}", event.block_height),
            1,
            LedgerEvent::MerkleRootGenerated(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::MerkleRootGenerated(deserialized_event) = deserialized.event_data {
            assert_eq!(event.block_height, deserialized_event.block_height);
            assert_eq!(event.root_hash, deserialized_event.root_hash);
            assert_eq!(event.transaction_count, deserialized_event.transaction_count);
            assert_eq!(event.previous_root_hash, deserialized_event.previous_root_hash);
        } else {
            panic!("Expected MerkleRootGenerated event");
        }
    }

    fn test_digest_published_event(serializer: &EventSerializer) {
        let digest = LedgerDigest {
            root_hash: "digest_root_hash_12345".to_string(),
            block_height: 12345,
            timestamp: chrono::Utc::now(),
            transaction_count: 5000,
        };
        
        let event = DigestPublishedEvent {
            digest: digest.clone(),
            storage_location: "s3://ledger-digests/2024/01/15/digest.json".to_string(),
            published_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            "digest-publication".to_string(),
            1,
            LedgerEvent::DigestPublished(event.clone()),
            EventMetadata::default(),
        );
        
        let serialized = serializer.serialize(&envelope).unwrap();
        let deserialized = serializer.deserialize(&serialized).unwrap();
        
        if let LedgerEvent::DigestPublished(deserialized_event) = deserialized.event_data {
            assert_eq!(event.digest.transaction_count, deserialized_event.digest.transaction_count);
            assert_eq!(event.digest.block_height, deserialized_event.digest.block_height);
            assert_eq!(event.digest.root_hash, deserialized_event.digest.root_hash);
            assert_eq!(event.storage_location, deserialized_event.storage_location);
        } else {
            panic!("Expected DigestPublished event");
        }
    }

    #[test]
    fn test_compression_efficiency() {
        let serializer_uncompressed = EventSerializer::new(false);
        let serializer_compressed = EventSerializer::new(true);
        
        // Create a large event with lots of metadata
        let mut large_tags = HashMap::new();
        for i in 0..100 {
            large_tags.insert(format!("tag_{}", i), format!("value_{}_with_some_longer_content", i));
        }
        
        let event = AccountCreatedEvent {
            account_id: AccountId::new(),
            account_type: AccountType::Customer,
            currency: Currency::usd(),
            initial_balance: Amount::new(1000000),
            metadata: AccountMetadata {
                account_type: AccountType::Customer,
                currency: Currency::usd(),
                tags: large_tags,
                regulatory_flags: (0..50).map(|i| format!("FLAG_{}", i)).collect(),
            },
            created_at: chrono::Utc::now(),
        };
        
        let envelope = EventEnvelope::new(
            event.account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        let uncompressed = serializer_uncompressed.serialize(&envelope).unwrap();
        let compressed = serializer_compressed.serialize(&envelope).unwrap();
        
        println!("Uncompressed size: {} bytes", uncompressed.len());
        println!("Compressed size: {} bytes", compressed.len());
        
        // Verify compression actually reduces size for large events
        assert!(compressed.len() < uncompressed.len());
        
        // Verify both can be deserialized correctly
        let uncompressed_result = serializer_uncompressed.deserialize(&uncompressed).unwrap();
        let compressed_result = serializer_compressed.deserialize(&compressed).unwrap();
        
        assert_eq!(envelope.event_id, uncompressed_result.event_id);
        assert_eq!(envelope.event_id, compressed_result.event_id);
    }

    #[test]
    fn test_key_generation_ordering() {
        let now = chrono::Utc::now();
        
        // Create events with different timestamps
        let event1 = create_test_envelope(now - chrono::Duration::seconds(10));
        let event2 = create_test_envelope(now - chrono::Duration::seconds(5));
        let event3 = create_test_envelope(now);
        
        let key1 = generate_event_key(&event1);
        let key2 = generate_event_key(&event2);
        let key3 = generate_event_key(&event3);
        
        // Keys should be ordered by timestamp
        assert!(key1 < key2);
        assert!(key2 < key3);
        
        // All keys should be 24 bytes (8 timestamp + 16 UUID)
        assert_eq!(key1.len(), 24);
        assert_eq!(key2.len(), 24);
        assert_eq!(key3.len(), 24);
    }

    fn create_test_envelope(timestamp: chrono::DateTime<chrono::Utc>) -> EventEnvelope {
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
            created_at: timestamp,
        };
        
        let mut envelope = EventEnvelope::new(
            account_id.0.to_string(),
            1,
            LedgerEvent::AccountCreated(event),
            EventMetadata::default(),
        );
        
        // Override timestamp for testing
        envelope.timestamp = timestamp;
        envelope
    }

    #[test]
    fn test_format_versioning() {
        let serializer = EventSerializer::new(false);
        let envelope = create_test_envelope(chrono::Utc::now());
        
        let serialized = serializer.serialize(&envelope).unwrap();
        
        // Verify the envelope can be deserialized correctly
        let deserialized = serializer.deserialize(&serialized).unwrap();
        assert_eq!(envelope.event_id, deserialized.event_id);
        assert_eq!(envelope.aggregate_id, deserialized.aggregate_id);
        assert_eq!(envelope.aggregate_version, deserialized.aggregate_version);
    }

    #[test]
    fn test_hash_calculation_consistency() {
        let envelope = create_test_envelope(chrono::Utc::now());
        
        // Hash should be consistent across multiple calls
        let hash1 = envelope.calculate_hash();
        let hash2 = envelope.calculate_hash();
        assert_eq!(hash1, hash2);
        
        // Hash should be 64 characters (SHA-256 hex)
        assert_eq!(hash1.len(), 64);
        
        // Different events should have different hashes
        let envelope2 = create_test_envelope(chrono::Utc::now());
        let hash3 = envelope2.calculate_hash();
        assert_ne!(hash1, hash3);
    }
}