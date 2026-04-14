//! Scanner thread abstraction for batched, channel-based pipelines.

use std::thread::JoinHandle;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender};

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

/// Sender wrapper that handles batching internally.
///
/// Items are accumulated until `batch_size` is reached, then sent as a batch.
/// Any remaining items are flushed when the sender is dropped.
pub struct BatchSender<T: Send + Sync + 'static> {
    tx: Sender<Batch<T>>,
    batch_size: usize,
    pending: Vec<T>,
}

impl<T: Send + Sync + 'static> BatchSender<T> {
    fn new(tx: Sender<Batch<T>>, batch_size: usize) -> Self {
        Self {
            tx,
            batch_size,
            pending: Vec::with_capacity(batch_size),
        }
    }

    /// Add an item. Sends a full batch when `batch_size` is reached.
    pub fn send_item(&mut self, item: T) -> Result<()> {
        self.pending.push(item);
        if self.pending.len() >= self.batch_size {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        if !self.pending.is_empty() {
            let items = std::mem::replace(
                &mut self.pending,
                Vec::with_capacity(self.batch_size),
            );
            self.tx.send(Batch { items })?;
        }
        Ok(())
    }
}

impl<T: Send + Sync + 'static> Drop for BatchSender<T> {
    fn drop(&mut self) {
        // Flush remaining items; ignore send errors (channel may be closed).
        let _ = self.flush();
    }
}

/// Spawn a scanner thread that calls `producer` to emit items, batches them,
/// and sends via a bounded channel.
///
/// The producer receives a [`BatchSender`] to accumulate items. Returns the
/// receiver end and the thread's join handle.
pub fn start_scanner<T, F>(
    producer: F,
    config: BatchConfig,
) -> (Receiver<Batch<T>>, JoinHandle<Result<usize>>)
where
    T: Send + Sync + 'static,
    F: FnOnce(&mut BatchSender<T>) -> Result<usize> + Send + 'static,
{
    let (tx, rx) = crossbeam_channel::bounded::<Batch<T>>(config.channel_capacity);
    let handle = std::thread::spawn(move || {
        let mut sender = BatchSender::new(tx, config.batch_size);
        producer(&mut sender)
        // sender dropped here, flushing any remaining items
    });
    (rx, handle)
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

    #[test]
    fn test_start_scanner_basic() {
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };
        let (rx, handle) = start_scanner(
            |tx| {
                // Produce 7 items -> batches of [3, 3, 1]
                for i in 0..7u32 {
                    tx.send_item(i)?;
                }
                Ok(7)
            },
            config,
        );

        let batches: Vec<Batch<u32>> = rx.iter().collect();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].items.len(), 3);
        assert_eq!(batches[1].items.len(), 3);
        assert_eq!(batches[2].items.len(), 1);
        let total: usize = batches.iter().map(|b| b.items.len()).sum();
        assert_eq!(total, 7);
        assert_eq!(handle.join().unwrap().unwrap(), 7);
    }

    #[test]
    fn test_start_scanner_empty_producer() {
        let config = BatchConfig {
            batch_size: 10,
            channel_capacity: 4,
        };
        let (rx, handle) = start_scanner(
            |_tx| {
                // Produce nothing
                Ok(0)
            },
            config,
        );

        let batches: Vec<Batch<u32>> = rx.iter().collect();
        assert_eq!(batches.len(), 0);
        assert_eq!(handle.join().unwrap().unwrap(), 0);
    }

    #[test]
    fn test_start_scanner_exact_batch_boundary() {
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };
        let (rx, handle) = start_scanner(
            |tx| {
                for i in 0..6u32 {
                    tx.send_item(i)?;
                }
                Ok(6)
            },
            config,
        );

        let batches: Vec<Batch<u32>> = rx.iter().collect();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].items, vec![0, 1, 2]);
        assert_eq!(batches[1].items, vec![3, 4, 5]);
        assert_eq!(handle.join().unwrap().unwrap(), 6);
    }
}
