//! Rayon-based order-preserving parallel batch processing.

use rayon::prelude::*;

use super::scanner::Batch;

/// Apply `transform` to each item in the batch via rayon, preserving order.
pub fn process_batch<T, R, F>(batch: &Batch<T>, transform: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync + Send,
{
    batch.items.par_iter().map(transform).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::scanner::Batch;

    #[test]
    fn test_process_batch_preserves_order() {
        let batch = Batch {
            items: vec![3, 1, 4, 1, 5],
        };
        let results = process_batch(&batch, |&x| x * 2);
        assert_eq!(results, vec![6, 2, 8, 2, 10]);
    }

    #[test]
    fn test_process_batch_empty() {
        let batch: Batch<i32> = Batch { items: vec![] };
        let results = process_batch(&batch, |&x| x * 2);
        assert!(results.is_empty());
    }

    #[test]
    fn test_process_batch_type_transform() {
        let batch = Batch {
            items: vec![1, 2, 3],
        };
        let results = process_batch(&batch, |&x| format!("item-{}", x));
        assert_eq!(results, vec!["item-1", "item-2", "item-3"]);
    }
}
