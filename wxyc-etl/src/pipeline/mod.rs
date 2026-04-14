//! Three-stage parallel pipeline framework.
//!
//! Provides a generic scanner → processor → writer pipeline with bounded
//! channels for backpressure and rayon for order-preserving parallel processing.

pub mod scanner;
pub mod processor;
pub mod writer;
pub mod runner;
