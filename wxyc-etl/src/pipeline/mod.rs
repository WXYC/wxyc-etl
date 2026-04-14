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

pub mod scanner;
pub mod processor;
pub mod writer;
pub mod runner;

// Re-exports for ergonomic `use wxyc_etl::pipeline::*` imports.
pub use scanner::{Batch, BatchConfig, BatchSender, ByteBatch, start_scanner, start_byte_scanner};
pub use processor::{process_batch, process_byte_batch};
pub use writer::PipelineOutput;
pub use runner::{DedupConfig, PipelineStats, run_pipeline, run_byte_pipeline};
