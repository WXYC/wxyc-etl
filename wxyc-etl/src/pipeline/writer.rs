//! Sequential output trait for pipeline writers.

use anyhow::Result;

/// Trait for writing pipeline items sequentially.
///
/// Generalizes `ReleaseOutput` from `discogs-xml-converter/output.rs`,
/// made generic over the item type.
pub trait PipelineOutput<T> {
    /// Write a single item to the output.
    fn write_item(&mut self, item: &T) -> Result<()>;

    /// Flush any buffered data to the output target.
    fn flush(&mut self) -> Result<()>;

    /// Finalize: flush remaining data and perform any post-processing.
    fn finish(&mut self) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockOutput {
        items: Vec<i32>,
        flushed: bool,
        finished: bool,
    }

    impl MockOutput {
        fn new() -> Self {
            Self {
                items: Vec::new(),
                flushed: false,
                finished: false,
            }
        }
    }

    impl PipelineOutput<i32> for MockOutput {
        fn write_item(&mut self, item: &i32) -> Result<()> {
            self.items.push(*item);
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            self.flushed = true;
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            self.finished = true;
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_output_trait() {
        let mut output = MockOutput::new();
        output.write_item(&42).unwrap();
        output.write_item(&99).unwrap();
        output.flush().unwrap();
        output.finish().unwrap();

        assert_eq!(output.items, vec![42, 99]);
        assert!(output.flushed);
        assert!(output.finished);
    }

    #[test]
    fn test_pipeline_output_is_object_safe() {
        let mut output = MockOutput::new();
        let dyn_output: &mut dyn PipelineOutput<i32> = &mut output;
        dyn_output.write_item(&1).unwrap();
        assert_eq!(output.items, vec![1]);
    }
}
