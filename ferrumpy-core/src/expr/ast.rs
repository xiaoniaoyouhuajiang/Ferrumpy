//! AST definitions for supported expressions

use serde::{Deserialize, Serialize};

/// Supported expression AST
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Expr {
    /// Variable or path: a, a.b, a[0].c
    Path(Vec<PathSegment>),

    /// Binary operation: a + b
    Binary {
        left: Box<Expr>,
        op: BinOp,
        right: Box<Expr>,
    },

    /// Unary operation: -a, !b, *ptr
    Unary { op: UnaryOp, expr: Box<Expr> },

    /// Literal: 42, 3.14, true, "hello"
    Literal(Literal),

    /// Parenthesized: (a + b)
    Paren(Box<Expr>),

    /// Type cast: a as i64
    Cast { expr: Box<Expr>, ty: String },
}

/// Path segment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PathSegment {
    /// Identifier: foo
    Ident(String),
    /// Index: [0]
    Index(usize),
    /// Tuple index: .0
    TupleIndex(usize),
    /// Dereference: *
    Deref,
    /// Reference: &
    Ref,
}

/// Binary operators
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum BinOp {
    // Arithmetic
    Add, // +
    Sub, // -
    Mul, // *
    Div, // /
    Rem, // %

    // Comparison
    Eq, // ==
    Ne, // !=
    Lt, // <
    Le, // <=
    Gt, // >
    Ge, // >=

    // Logical
    And, // &&
    Or,  // ||

    // Bitwise
    BitAnd, // &
    BitOr,  // |
    BitXor, // ^
    Shl,    // <<
    Shr,    // >>
}

impl BinOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Rem => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        }
    }
}

/// Unary operators
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,   // -
    Not,   // !
    Deref, // *
    Ref,   // &
}

impl UnaryOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::Deref => "*",
            UnaryOp::Ref => "&",
        }
    }
}

/// Literal values
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Literal {
    Int(i128),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
}
