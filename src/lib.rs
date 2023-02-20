//! A collection of utilities for writing software.

#![warn(missing_docs)]

#[cfg(feature = "jobs")]
pub mod jobs;

#[cfg(feature = "hash")]
pub mod hash;

#[cfg(feature = "flags")]
pub mod flags;

pub mod metrics;
pub mod trace;

pub use metrics::MetricsServer;
pub use trace::{init as trace_init, TracingConfig};
