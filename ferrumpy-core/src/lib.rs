//! FerrumPy Core Library
#![allow(clippy::all)]
//!
//! Core functionality for Rust debugger enhancements:
//! - DWARF type processing
//! - rust-analyzer integration
//! - Expression parsing and evaluation (Phase 3)
//! - Auto-Lib Generation (Phase 4)
//! - Embedded REPL (Phase 4.5)
//! - Python bindings (pyo3, optional)

pub mod dwarf;
pub mod expr;
pub mod libgen;
pub mod lsp;
pub mod protocol;
pub mod repl;

#[cfg(feature = "python")]
mod python;

pub use expr::{parse_expr, EvalError, Evaluator, Expr, Value};
pub use libgen::{generate_lib, GeneratedLib, LibGenConfig};
pub use lsp::CompletionItem;
pub use protocol::{Request, Response};
pub use repl::ReplSession;
