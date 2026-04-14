//! Scanner thread abstraction for batched, channel-based pipelines.

/// Configuration for batch size and channel capacity.
pub struct BatchConfig {
    /// Number of items per batch (default: 256).
    pub batch_size: usize,
    /// Bounded channel capacity in batches (default: 64).
    pub channel_capacity: usize,
}

impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            batch_size: 256,
            channel_capacity: 64,
        }
    }
}

/// A batch of items, generic over item type `T`.
pub struct Batch<T> {
    pub items: Vec<T>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_config_defaults() {
        let config = BatchConfig::default();
        assert_eq!(config.batch_size, 256);
        assert_eq!(config.channel_capacity, 64);
    }

    #[test]
    fn test_batch_items_accessible() {
        let batch = Batch {
            items: vec![1, 2, 3],
        };
        assert_eq!(batch.items.len(), 3);
        assert_eq!(batch.items[0], 1);
    }
}
