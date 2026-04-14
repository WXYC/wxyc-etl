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

    // --- Integration tests: PipelineOutput contract ---

    /// A string-typed PipelineOutput that tracks call order.
    struct TrackedStringOutput {
        items: Vec<String>,
        flush_count: usize,
        finish_count: usize,
    }

    impl TrackedStringOutput {
        fn new() -> Self {
            Self {
                items: Vec::new(),
                flush_count: 0,
                finish_count: 0,
            }
        }
    }

    impl PipelineOutput<String> for TrackedStringOutput {
        fn write_item(&mut self, item: &String) -> Result<()> {
            self.items.push(item.clone());
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            self.flush_count += 1;
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            self.finish_count += 1;
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_output_string_type() {
        let mut output = TrackedStringOutput::new();
        output.write_item(&"Autechre".to_string()).unwrap();
        output.write_item(&"Stereolab".to_string()).unwrap();
        output.write_item(&"Cat Power".to_string()).unwrap();
        output.flush().unwrap();
        output.finish().unwrap();

        assert_eq!(output.items, vec!["Autechre", "Stereolab", "Cat Power"]);
        assert_eq!(output.flush_count, 1);
        assert_eq!(output.finish_count, 1);
    }

    #[test]
    fn test_pipeline_output_multiple_flushes() {
        let mut output = TrackedStringOutput::new();
        output.write_item(&"Juana Molina".to_string()).unwrap();
        output.flush().unwrap();
        output.write_item(&"Sessa".to_string()).unwrap();
        output.flush().unwrap();
        output.finish().unwrap();

        assert_eq!(output.items, vec!["Juana Molina", "Sessa"]);
        assert_eq!(output.flush_count, 2);
        assert_eq!(output.finish_count, 1);
    }

    #[test]
    fn test_pipeline_output_empty_workflow() {
        // Flushing and finishing without any writes should be valid
        let mut output = MockOutput::new();
        output.flush().unwrap();
        output.finish().unwrap();

        assert!(output.items.is_empty());
        assert!(output.flushed);
        assert!(output.finished);
    }

    /// An error-producing output for testing error propagation.
    struct FailingOutput {
        fail_on_write: usize,
        write_count: usize,
    }

    impl FailingOutput {
        fn new(fail_on_write: usize) -> Self {
            Self {
                fail_on_write,
                write_count: 0,
            }
        }
    }

    impl PipelineOutput<i32> for FailingOutput {
        fn write_item(&mut self, _item: &i32) -> Result<()> {
            self.write_count += 1;
            if self.write_count >= self.fail_on_write {
                anyhow::bail!("write failed at count {}", self.write_count);
            }
            Ok(())
        }

        fn flush(&mut self) -> Result<()> {
            Ok(())
        }

        fn finish(&mut self) -> Result<()> {
            Ok(())
        }
    }

    #[test]
    fn test_pipeline_output_error_propagation() {
        let mut output = FailingOutput::new(3);
        output.write_item(&1).unwrap();
        output.write_item(&2).unwrap();
        let result = output.write_item(&3);
        assert!(result.is_err());
        assert_eq!(output.write_count, 3);
    }
}
