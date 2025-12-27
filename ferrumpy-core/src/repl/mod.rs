//! REPL Module
//!
//! Provides an embedded Rust REPL using evcxr.
//! This allows ferrumpy to run Rust expressions with captured debug state.

mod session;

pub use session::ReplSession;
