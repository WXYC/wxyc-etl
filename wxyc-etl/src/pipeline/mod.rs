//! Three-stage parallel pipeline framework.
//!
//! Provides a generic scanner → processor → writer pipeline with bounded
//! channels for backpressure and rayon for order-preserving parallel processing.
//!
//! # Architecture
//!
//! 1. **Scanner** — a background thread reads input and sends batches via a
//!    bounded crossbeam channel (backpressure).
//! 2. **Processor** — rayon workers transform batches in parallel, preserving
//!    input order.
//! 3. **Writer** — a sequential [`PipelineOutput`] consumer writes results.
//!
//! # Usage
//!
//! ```ignore
//! use wxyc_etl::pipeline::*;
//!
//! let config = BatchConfig::default();
//! let (rx, handle) = start_scanner(|tx| { /* produce items */ Ok(0) }, config);
//! let mut output = /* impl PipelineOutput<R> */;
//! let stats = run_pipeline(rx, handle, |item| Some(transform(item)), &mut output)?;
//! ```

pub mod processor;
pub mod runner;
pub mod scanner;
pub mod writer;

// Re-exports for ergonomic `use wxyc_etl::pipeline::*` imports.
pub use processor::{process_batch, process_byte_batch};
pub use runner::{run_byte_pipeline, run_pipeline, DedupConfig, PipelineStats};
pub use scanner::{start_byte_scanner, start_scanner, Batch, BatchConfig, BatchSender, ByteBatch};
pub use writer::PipelineOutput;
