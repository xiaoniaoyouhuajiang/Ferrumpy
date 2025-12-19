//! Expression evaluation module
//! 
//! Provides parsing and evaluation of Rust expressions for debugging.

pub mod ast;
pub mod parser;
pub mod value;
pub mod eval;
pub mod error;

pub use ast::Expr;
pub use value::Value;
pub use error::EvalError;
pub use parser::parse_expr;
pub use eval::Evaluator;
