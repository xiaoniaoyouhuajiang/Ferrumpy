//! Expression parser using syn
//!
//! Converts Rust expression strings to our AST.

use syn::{
    Expr as SynExpr, ExprBinary, ExprCast, ExprField, ExprIndex, ExprLit, ExprParen, ExprPath,
    ExprUnary,
};

use super::ast::{BinOp, Expr, Literal, PathSegment, UnaryOp};
use super::error::EvalError;

/// Parse an expression string into our AST
pub fn parse_expr(input: &str) -> Result<Expr, EvalError> {
    let syn_expr: SynExpr = syn::parse_str(input).map_err(|e| EvalError::ParseError {
        message: e.to_string(),
    })?;

    convert_expr(&syn_expr)
}

/// Convert syn expression to our AST
fn convert_expr(expr: &SynExpr) -> Result<Expr, EvalError> {
    match expr {
        // Binary operations: a + b
        SynExpr::Binary(ExprBinary {
            left, op, right, ..
        }) => {
            let bin_op = convert_binop(op)?;
            Ok(Expr::Binary {
                left: Box::new(convert_expr(left)?),
                op: bin_op,
                right: Box::new(convert_expr(right)?),
            })
        }

        // Unary operations: -a, !b, *ptr
        SynExpr::Unary(ExprUnary { op, expr, .. }) => {
            let unary_op = convert_unary_op(op)?;
            Ok(Expr::Unary {
                op: unary_op,
                expr: Box::new(convert_expr(expr)?),
            })
        }

        // Literals: 42, 3.14, true
        SynExpr::Lit(ExprLit { lit, .. }) => {
            let literal = convert_literal(lit)?;
            Ok(Expr::Literal(literal))
        }

        // Path: a, a::b
        SynExpr::Path(ExprPath { path, .. }) => {
            let segments = path
                .segments
                .iter()
                .map(|seg| PathSegment::Ident(seg.ident.to_string()))
                .collect();
            Ok(Expr::Path(segments))
        }

        // Field access: a.b
        SynExpr::Field(ExprField { base, member, .. }) => {
            let mut segments = extract_path_segments(base)?;

            match member {
                syn::Member::Named(ident) => {
                    segments.push(PathSegment::Ident(ident.to_string()));
                }
                syn::Member::Unnamed(index) => {
                    segments.push(PathSegment::TupleIndex(index.index as usize));
                }
            }

            Ok(Expr::Path(segments))
        }

        // Index: a[0]
        SynExpr::Index(ExprIndex { expr, index, .. }) => {
            let mut segments = extract_path_segments(expr)?;

            // Index must be a literal integer
            if let SynExpr::Lit(ExprLit {
                lit: syn::Lit::Int(lit_int),
                ..
            }) = index.as_ref()
            {
                let idx = lit_int
                    .base10_parse::<usize>()
                    .map_err(|e| EvalError::ParseError {
                        message: e.to_string(),
                    })?;
                segments.push(PathSegment::Index(idx));
                Ok(Expr::Path(segments))
            } else {
                Err(EvalError::unsupported("dynamic index expressions"))
            }
        }

        // Parenthesized: (a + b)
        SynExpr::Paren(ExprParen { expr, .. }) => Ok(Expr::Paren(Box::new(convert_expr(expr)?))),

        // Cast: a as i64
        SynExpr::Cast(ExprCast { expr, ty, .. }) => {
            let type_str = quote::quote!(#ty).to_string();
            Ok(Expr::Cast {
                expr: Box::new(convert_expr(expr)?),
                ty: type_str,
            })
        }

        // Reference: &a
        SynExpr::Reference(r) => Ok(Expr::Unary {
            op: UnaryOp::Ref,
            expr: Box::new(convert_expr(&r.expr)?),
        }),

        // Function calls - not supported
        SynExpr::Call(_) => Err(EvalError::unsupported("function calls")),

        // Method calls - not supported
        SynExpr::MethodCall(_) => Err(EvalError::unsupported("method calls")),

        // Closures - not supported
        SynExpr::Closure(_) => Err(EvalError::unsupported("closures")),

        // Block expressions - not supported
        SynExpr::Block(_) => Err(EvalError::unsupported("block expressions")),

        // If expressions - not supported
        SynExpr::If(_) => Err(EvalError::unsupported("if expressions")),

        // Match expressions - not supported
        SynExpr::Match(_) => Err(EvalError::unsupported("match expressions")),

        // Other unsupported expressions
        other => {
            let debug_str = format!("{:?}", other);
            let kind = debug_str.split('(').next().unwrap_or("unknown").to_string();
            Err(EvalError::unsupported(kind))
        }
    }
}

/// Extract path segments from nested field/index expressions
fn extract_path_segments(expr: &SynExpr) -> Result<Vec<PathSegment>, EvalError> {
    match expr {
        SynExpr::Path(ExprPath { path, .. }) => Ok(path
            .segments
            .iter()
            .map(|seg| PathSegment::Ident(seg.ident.to_string()))
            .collect()),
        SynExpr::Field(ExprField { base, member, .. }) => {
            let mut segments = extract_path_segments(base)?;
            match member {
                syn::Member::Named(ident) => {
                    segments.push(PathSegment::Ident(ident.to_string()));
                }
                syn::Member::Unnamed(index) => {
                    segments.push(PathSegment::TupleIndex(index.index as usize));
                }
            }
            Ok(segments)
        }
        SynExpr::Index(ExprIndex { expr, index, .. }) => {
            let mut segments = extract_path_segments(expr)?;
            if let SynExpr::Lit(ExprLit {
                lit: syn::Lit::Int(lit_int),
                ..
            }) = index.as_ref()
            {
                let idx = lit_int
                    .base10_parse::<usize>()
                    .map_err(|e| EvalError::ParseError {
                        message: e.to_string(),
                    })?;
                segments.push(PathSegment::Index(idx));
                Ok(segments)
            } else {
                Err(EvalError::unsupported("dynamic index"))
            }
        }
        SynExpr::Unary(ExprUnary {
            op: syn::UnOp::Deref(_),
            expr,
            ..
        }) => {
            let mut segments = extract_path_segments(expr)?;
            segments.insert(0, PathSegment::Deref);
            Ok(segments)
        }
        _ => Err(EvalError::unsupported("complex path expression")),
    }
}

/// Convert syn binary operator to our BinOp
fn convert_binop(op: &syn::BinOp) -> Result<BinOp, EvalError> {
    match op {
        syn::BinOp::Add(_) => Ok(BinOp::Add),
        syn::BinOp::Sub(_) => Ok(BinOp::Sub),
        syn::BinOp::Mul(_) => Ok(BinOp::Mul),
        syn::BinOp::Div(_) => Ok(BinOp::Div),
        syn::BinOp::Rem(_) => Ok(BinOp::Rem),
        syn::BinOp::Eq(_) => Ok(BinOp::Eq),
        syn::BinOp::Ne(_) => Ok(BinOp::Ne),
        syn::BinOp::Lt(_) => Ok(BinOp::Lt),
        syn::BinOp::Le(_) => Ok(BinOp::Le),
        syn::BinOp::Gt(_) => Ok(BinOp::Gt),
        syn::BinOp::Ge(_) => Ok(BinOp::Ge),
        syn::BinOp::And(_) => Ok(BinOp::And),
        syn::BinOp::Or(_) => Ok(BinOp::Or),
        syn::BinOp::BitAnd(_) => Ok(BinOp::BitAnd),
        syn::BinOp::BitOr(_) => Ok(BinOp::BitOr),
        syn::BinOp::BitXor(_) => Ok(BinOp::BitXor),
        syn::BinOp::Shl(_) => Ok(BinOp::Shl),
        syn::BinOp::Shr(_) => Ok(BinOp::Shr),
        _ => Err(EvalError::unsupported("assignment operators")),
    }
}

/// Convert syn unary operator to our UnaryOp
fn convert_unary_op(op: &syn::UnOp) -> Result<UnaryOp, EvalError> {
    match op {
        syn::UnOp::Neg(_) => Ok(UnaryOp::Neg),
        syn::UnOp::Not(_) => Ok(UnaryOp::Not),
        syn::UnOp::Deref(_) => Ok(UnaryOp::Deref),
        _ => Err(EvalError::unsupported("unknown unary operator")),
    }
}

/// Convert syn literal to our Literal
fn convert_literal(lit: &syn::Lit) -> Result<Literal, EvalError> {
    match lit {
        syn::Lit::Int(i) => {
            let value = i
                .base10_parse::<i128>()
                .map_err(|e| EvalError::ParseError {
                    message: e.to_string(),
                })?;
            Ok(Literal::Int(value))
        }
        syn::Lit::Float(f) => {
            let value = f.base10_parse::<f64>().map_err(|e| EvalError::ParseError {
                message: e.to_string(),
            })?;
            Ok(Literal::Float(value))
        }
        syn::Lit::Bool(b) => Ok(Literal::Bool(b.value)),
        syn::Lit::Char(c) => Ok(Literal::Char(c.value())),
        syn::Lit::Str(s) => Ok(Literal::String(s.value())),
        _ => Err(EvalError::unsupported("byte literals")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_path() {
        let expr = parse_expr("foo").unwrap();
        assert!(matches!(expr, Expr::Path(_)));
    }

    #[test]
    fn test_parse_field_access() {
        let expr = parse_expr("foo.bar").unwrap();
        if let Expr::Path(segments) = expr {
            assert_eq!(segments.len(), 2);
        } else {
            panic!("Expected Path");
        }
    }

    #[test]
    fn test_parse_binary() {
        let expr = parse_expr("a + b").unwrap();
        assert!(matches!(expr, Expr::Binary { .. }));
    }

    #[test]
    fn test_parse_literal() {
        let expr = parse_expr("42").unwrap();
        assert!(matches!(expr, Expr::Literal(Literal::Int(42))));
    }

    #[test]
    fn test_unsupported_function_call() {
        let result = parse_expr("foo()");
        assert!(matches!(
            result,
            Err(EvalError::UnsupportedExpression { .. })
        ));
    }

    #[test]
    fn test_unsupported_method_call() {
        let result = parse_expr("a.len()");
        assert!(matches!(
            result,
            Err(EvalError::UnsupportedExpression { .. })
        ));
    }
}
