//! REPL Module
//!
//! Provides an embedded Rust REPL using evcxr.
//! This allows ferrumpy to run Rust expressions with captured debug state.

mod scan;
mod session;

pub use scan::FragmentValidity;
pub use session::ReplSession;
