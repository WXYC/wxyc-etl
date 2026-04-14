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

/// A batch of byte slices sharing a contiguous buffer.
///
/// Equivalent to `ReleaseBatch` in `discogs-xml-converter`: a single `Vec<u8>`
/// holds concatenated data, and `offsets` records `(start, end)` pairs for each
/// logical item.
pub struct ByteBatch {
    /// Buffer containing concatenated raw bytes for multiple items.
    pub data: Vec<u8>,
    /// `(start, end)` byte offsets into `data` for each item.
    pub offsets: Vec<(usize, usize)>,
}

impl ByteBatch {
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            offsets: Vec::new(),
        }
    }

    /// Append a byte slice to this batch.
    pub fn push_slice(&mut self, slice: &[u8]) {
        let start = self.data.len();
        self.data.extend_from_slice(slice);
        self.offsets.push((start, self.data.len()));
    }

    /// Number of items in this batch.
    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Whether this batch is empty.
    pub fn is_empty(&self) -> bool {
        self.offsets.is_empty()
    }

    /// Get the byte slice for item at `index`.
    pub fn get(&self, index: usize) -> &[u8] {
        let (start, end) = self.offsets[index];
        &self.data[start..end]
    }

    /// Build a `ByteBatch` from a slice of byte slices (convenience for tests).
    pub fn from_slices(slices: &[&[u8]]) -> Self {
        let mut batch = Self::new();
        for s in slices {
            batch.push_slice(s);
        }
        batch
    }
}

/// Spawn a byte scanner thread.
///
/// The producer receives a raw `Sender<ByteBatch>` since byte batches are
/// typically constructed by the scanner itself (e.g., finding XML boundaries).
/// Returns the receiver and join handle.
pub fn start_byte_scanner<F>(
    scanner_fn: F,
    config: BatchConfig,
) -> (Receiver<ByteBatch>, JoinHandle<Result<usize>>)
where
    F: FnOnce(&Sender<ByteBatch>) -> Result<usize> + Send + 'static,
{
    let (tx, rx) = crossbeam_channel::bounded::<ByteBatch>(config.channel_capacity);
    let handle = std::thread::spawn(move || scanner_fn(&tx));
    (rx, handle)
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

    #[test]
    fn test_byte_batch_push_and_access() {
        let mut batch = ByteBatch::new();
        batch.push_slice(b"hello");
        batch.push_slice(b"world");

        assert_eq!(batch.len(), 2);
        assert_eq!(batch.get(0), b"hello");
        assert_eq!(batch.get(1), b"world");
    }

    #[test]
    fn test_start_byte_scanner() {
        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 4,
        };

        let (rx, handle) = start_byte_scanner(
            |tx| {
                tx.send(ByteBatch::from_slices(&[b"first", b"second"]))?;
                tx.send(ByteBatch::from_slices(&[b"third"]))?;
                Ok(3)
            },
            config,
        );

        let batches: Vec<ByteBatch> = rx.iter().collect();
        assert_eq!(batches.len(), 2);
        assert_eq!(batches[0].len(), 2);
        assert_eq!(batches[0].get(0), b"first");
        assert_eq!(batches[0].get(1), b"second");
        assert_eq!(batches[1].len(), 1);
        assert_eq!(batches[1].get(0), b"third");
        assert_eq!(handle.join().unwrap().unwrap(), 3);
    }

    // --- Integration tests: batch accumulation, flush on drop, backpressure ---

    #[test]
    fn test_batch_sender_flush_on_drop() {
        // Items that don't fill a complete batch should still be sent when
        // the BatchSender is dropped (flush on drop).
        let (tx, rx) = crossbeam_channel::bounded::<Batch<String>>(4);
        {
            let mut sender = BatchSender::new(tx, 10); // batch_size=10
            // Send only 3 items, below the batch threshold
            sender.send_item("Autechre".to_string()).unwrap();
            sender.send_item("Stereolab".to_string()).unwrap();
            sender.send_item("Cat Power".to_string()).unwrap();
            // sender drops here, triggering flush
        }
        let batches: Vec<Batch<String>> = rx.iter().collect();
        assert_eq!(batches.len(), 1, "remaining items should be flushed on drop");
        assert_eq!(batches[0].items.len(), 3);
        assert_eq!(batches[0].items[0], "Autechre");
        assert_eq!(batches[0].items[1], "Stereolab");
        assert_eq!(batches[0].items[2], "Cat Power");
    }

    #[test]
    fn test_batch_sender_no_flush_when_empty() {
        // Dropping a BatchSender with no pending items should not send a batch
        let (tx, rx) = crossbeam_channel::bounded::<Batch<u32>>(4);
        {
            let _sender = BatchSender::new(tx, 10);
            // no items sent, sender drops here
        }
        let batches: Vec<Batch<u32>> = rx.iter().collect();
        assert!(batches.is_empty(), "empty sender should not flush a batch");
    }

    #[test]
    fn test_batch_sender_multiple_full_batches_plus_remainder() {
        // Verifies batch accumulation across multiple full batches with a remainder
        let (tx, rx) = crossbeam_channel::bounded::<Batch<u32>>(10);
        {
            let mut sender = BatchSender::new(tx, 4);
            // Send 11 items: 4 + 4 + 3
            for i in 0..11u32 {
                sender.send_item(i).unwrap();
            }
        }
        let batches: Vec<Batch<u32>> = rx.iter().collect();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].items, vec![0, 1, 2, 3]);
        assert_eq!(batches[1].items, vec![4, 5, 6, 7]);
        assert_eq!(batches[2].items, vec![8, 9, 10]);
    }

    #[test]
    fn test_batch_sender_single_item_batch() {
        // batch_size=1 should send each item as its own batch
        let (tx, rx) = crossbeam_channel::bounded::<Batch<&str>>(10);
        {
            let mut sender = BatchSender::new(tx, 1);
            sender.send_item("Juana Molina").unwrap();
            sender.send_item("Sessa").unwrap();
            sender.send_item("Anne Gillis").unwrap();
        }
        let batches: Vec<Batch<&str>> = rx.iter().collect();
        assert_eq!(batches.len(), 3);
        assert_eq!(batches[0].items, vec!["Juana Molina"]);
        assert_eq!(batches[1].items, vec!["Sessa"]);
        assert_eq!(batches[2].items, vec!["Anne Gillis"]);
    }

    #[test]
    fn test_channel_backpressure() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::Arc;

        // With channel_capacity=1, the producer should block after the first
        // batch until the consumer reads it
        let sent_count = Arc::new(AtomicUsize::new(0));
        let sent_clone = sent_count.clone();

        let config = BatchConfig {
            batch_size: 2,
            channel_capacity: 1,
        };

        let (rx, handle) = start_scanner(
            move |tx| {
                for i in 0..10u32 {
                    tx.send_item(i)?;
                    sent_clone.store((i + 1) as usize, Ordering::SeqCst);
                }
                Ok(10)
            },
            config,
        );

        // Consume all batches
        let mut total_items = 0usize;
        for batch in rx.iter() {
            total_items += batch.items.len();
        }
        assert_eq!(total_items, 10);
        assert_eq!(handle.join().unwrap().unwrap(), 10);
        assert_eq!(sent_count.load(Ordering::SeqCst), 10);
    }

    #[test]
    fn test_byte_batch_empty() {
        let batch = ByteBatch::new();
        assert!(batch.is_empty());
        assert_eq!(batch.len(), 0);
    }

    #[test]
    fn test_byte_batch_from_slices() {
        let wxyc_data: Vec<&[u8]> = vec![
            b"Autechre\tConfield",
            b"Stereolab\tAluminum Tunes",
            b"Cat Power\tMoon Pix",
        ];
        let batch = ByteBatch::from_slices(&wxyc_data);
        assert_eq!(batch.len(), 3);
        assert!(!batch.is_empty());
        assert_eq!(batch.get(0), b"Autechre\tConfield");
        assert_eq!(batch.get(1), b"Stereolab\tAluminum Tunes");
        assert_eq!(batch.get(2), b"Cat Power\tMoon Pix");
    }

    #[test]
    fn test_byte_batch_contiguous_buffer() {
        // Verify that all items share the same contiguous buffer
        let mut batch = ByteBatch::new();
        batch.push_slice(b"Juana Molina");
        batch.push_slice(b"DOGA");

        // Offsets should be contiguous
        assert_eq!(batch.offsets[0], (0, 12));  // "Juana Molina" = 12 bytes
        assert_eq!(batch.offsets[1], (12, 16)); // "DOGA" = 4 bytes
        assert_eq!(batch.data.len(), 16);
    }

    #[test]
    fn test_start_scanner_preserves_item_order() {
        // Verify that items come out in the same order they were sent,
        // across multiple batches
        let config = BatchConfig {
            batch_size: 3,
            channel_capacity: 4,
        };
        let wxyc_artists = vec![
            "Autechre", "Stereolab", "Cat Power", "Juana Molina",
            "Jessica Pratt", "Chuquimamani-Condori", "Sessa", "Anne Gillis",
        ];
        let expected: Vec<String> = wxyc_artists.iter().map(|s| s.to_string()).collect();

        let (rx, handle) = start_scanner(
            move |tx| {
                for artist in &wxyc_artists {
                    tx.send_item(artist.to_string())?;
                }
                Ok(wxyc_artists.len())
            },
            config,
        );

        let mut received: Vec<String> = Vec::new();
        for batch in rx.iter() {
            received.extend(batch.items);
        }
        assert_eq!(received, expected, "items should arrive in send order");
        assert_eq!(handle.join().unwrap().unwrap(), 8);
    }
}
