//! Expression evaluation module
//!
//! Provides parsing and evaluation of Rust expressions for debugging.

pub mod ast;
pub mod error;
pub mod eval;
pub mod parser;
pub mod value;

pub use ast::Expr;
pub use error::EvalError;
pub use eval::Evaluator;
pub use parser::parse_expr;
pub use value::Value;
