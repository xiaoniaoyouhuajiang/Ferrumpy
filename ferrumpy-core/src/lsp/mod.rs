//! LSP (Language Server Protocol) module
//!
//! Handles communication with rust-analyzer for code intelligence features.

mod client;
pub mod types;

pub use client::RustAnalyzerClient;
pub use types::{CompletionItem, CompletionKind};
