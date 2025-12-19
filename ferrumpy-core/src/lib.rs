//! FerrumPy Core Library
//!
//! Core functionality for Rust debugger enhancements:
//! - DWARF type processing
//! - rust-analyzer integration
//! - Expression parsing and evaluation (Phase 3)
//! - Python bindings (pyo3, optional)

pub mod dwarf;
pub mod expr;
pub mod lsp;
pub mod protocol;

#[cfg(feature = "python")]
mod python;

pub use expr::{parse_expr, EvalError, Evaluator, Expr, Value};
pub use lsp::CompletionItem;
pub use protocol::{Request, Response};
