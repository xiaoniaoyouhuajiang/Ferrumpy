//! Expression evaluator
//!
//! Evaluates expressions against a variable context.

use std::collections::HashMap;

use super::ast::{BinOp, Expr, Literal, PathSegment, UnaryOp};
use super::error::EvalError;
use super::value::Value;

/// Variable context for evaluation
pub type VarContext = HashMap<String, Value>;

/// Expression evaluator
pub struct Evaluator {
    /// Variables available in scope
    variables: VarContext,
}

impl Evaluator {
    pub fn new() -> Self {
        Self {
            variables: HashMap::new(),
        }
    }

    pub fn with_variables(variables: VarContext) -> Self {
        Self { variables }
    }

    /// Add or update a variable
    pub fn set_variable(&mut self, name: impl Into<String>, value: Value) {
        self.variables.insert(name.into(), value);
    }

    /// Evaluate an expression
    pub fn eval(&self, expr: &Expr) -> Result<Value, EvalError> {
        match expr {
            Expr::Path(segments) => self.eval_path(segments),
            Expr::Binary { left, op, right } => {
                let l = self.eval(left)?;
                let r = self.eval(right)?;
                self.apply_binop(&l, *op, &r)
            }
            Expr::Unary { op, expr } => {
                let v = self.eval(expr)?;
                self.apply_unary(*op, &v)
            }
            Expr::Literal(lit) => Ok(self.literal_to_value(lit)),
            Expr::Paren(inner) => self.eval(inner),
            Expr::Cast { expr, ty } => {
                let v = self.eval(expr)?;
                self.cast_value(&v, ty)
            }
        }
    }

    /// Evaluate a path expression
    fn eval_path(&self, segments: &[PathSegment]) -> Result<Value, EvalError> {
        if segments.is_empty() {
            return Err(EvalError::Internal("empty path".to_string()));
        }

        // First segment must be a variable name
        let first = &segments[0];
        let PathSegment::Ident(name) = first else {
            return Err(EvalError::Internal(
                "path must start with identifier".to_string(),
            ));
        };

        let value = self
            .variables
            .get(name)
            .ok_or_else(|| EvalError::unknown_var(name))?
            .clone();

        // For now, we only support simple variable lookups
        // Field access requires SBValue integration
        if segments.len() > 1 {
            return Err(EvalError::unsupported(
                "field access (requires runtime integration)",
            ));
        }

        Ok(value)
    }

    /// Convert literal to Value
    fn literal_to_value(&self, lit: &Literal) -> Value {
        match lit {
            // Default integer type is i32
            Literal::Int(v) => {
                if *v >= i32::MIN as i128 && *v <= i32::MAX as i128 {
                    Value::I32(*v as i32)
                } else if *v >= i64::MIN as i128 && *v <= i64::MAX as i128 {
                    Value::I64(*v as i64)
                } else {
                    Value::I128(*v)
                }
            }
            // Default float type is f64
            Literal::Float(v) => Value::F64(*v),
            Literal::Bool(v) => Value::Bool(*v),
            Literal::Char(v) => Value::Char(*v),
            Literal::String(v) => Value::String(v.clone()),
        }
    }

    /// Apply binary operator
    fn apply_binop(&self, left: &Value, op: BinOp, right: &Value) -> Result<Value, EvalError> {
        // Type checking: operands must be same type (strict Rust semantics)
        if left.type_name() != right.type_name() {
            return Err(EvalError::InvalidOperation {
                op: op.as_str().to_string(),
                left: left.type_name().to_string(),
                right: right.type_name().to_string(),
            });
        }

        match op {
            // Arithmetic operations
            BinOp::Add | BinOp::Sub | BinOp::Mul | BinOp::Div | BinOp::Rem => {
                self.apply_arithmetic(left, op, right)
            }
            // Comparison operations
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => {
                self.apply_comparison(left, op, right)
            }
            // Logical operations
            BinOp::And | BinOp::Or => self.apply_logical(left, op, right),
            // Bitwise operations
            BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                self.apply_bitwise(left, op, right)
            }
        }
    }

    fn apply_arithmetic(&self, left: &Value, op: BinOp, right: &Value) -> Result<Value, EvalError> {
        // Integer arithmetic
        if let (Some(l), Some(r)) = (left.to_i128(), right.to_i128()) {
            let result = match op {
                BinOp::Add => l
                    .checked_add(r)
                    .ok_or(EvalError::Internal("overflow".to_string()))?,
                BinOp::Sub => l
                    .checked_sub(r)
                    .ok_or(EvalError::Internal("overflow".to_string()))?,
                BinOp::Mul => l
                    .checked_mul(r)
                    .ok_or(EvalError::Internal("overflow".to_string()))?,
                BinOp::Div => {
                    if r == 0 {
                        return Err(EvalError::DivisionByZero);
                    }
                    l / r
                }
                BinOp::Rem => {
                    if r == 0 {
                        return Err(EvalError::DivisionByZero);
                    }
                    l % r
                }
                _ => unreachable!(),
            };

            // Return same type as operands
            return Ok(match left {
                Value::I8(_) => Value::I8(result as i8),
                Value::I16(_) => Value::I16(result as i16),
                Value::I32(_) => Value::I32(result as i32),
                Value::I64(_) => Value::I64(result as i64),
                Value::I128(_) => Value::I128(result),
                Value::Isize(_) => Value::Isize(result as isize),
                Value::U8(_) => Value::U8(result as u8),
                Value::U16(_) => Value::U16(result as u16),
                Value::U32(_) => Value::U32(result as u32),
                Value::U64(_) => Value::U64(result as u64),
                Value::U128(_) => Value::U128(result as u128),
                Value::Usize(_) => Value::Usize(result as usize),
                _ => unreachable!(),
            });
        }

        // Float arithmetic
        if let (Some(l), Some(r)) = (left.to_f64(), right.to_f64()) {
            let result = match op {
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => l / r,
                BinOp::Rem => l % r,
                _ => unreachable!(),
            };

            return Ok(match left {
                Value::F32(_) => Value::F32(result as f32),
                Value::F64(_) => Value::F64(result),
                _ => unreachable!(),
            });
        }

        Err(EvalError::InvalidOperation {
            op: op.as_str().to_string(),
            left: left.type_name().to_string(),
            right: right.type_name().to_string(),
        })
    }

    fn apply_comparison(&self, left: &Value, op: BinOp, right: &Value) -> Result<Value, EvalError> {
        // Integer comparison
        if let (Some(l), Some(r)) = (left.to_i128(), right.to_i128()) {
            let result = match op {
                BinOp::Eq => l == r,
                BinOp::Ne => l != r,
                BinOp::Lt => l < r,
                BinOp::Le => l <= r,
                BinOp::Gt => l > r,
                BinOp::Ge => l >= r,
                _ => unreachable!(),
            };
            return Ok(Value::Bool(result));
        }

        // Float comparison
        if let (Some(l), Some(r)) = (left.to_f64(), right.to_f64()) {
            let result = match op {
                BinOp::Eq => l == r,
                BinOp::Ne => l != r,
                BinOp::Lt => l < r,
                BinOp::Le => l <= r,
                BinOp::Gt => l > r,
                BinOp::Ge => l >= r,
                _ => unreachable!(),
            };
            return Ok(Value::Bool(result));
        }

        // Bool comparison
        if let (Some(l), Some(r)) = (left.to_bool(), right.to_bool()) {
            let result = match op {
                BinOp::Eq => l == r,
                BinOp::Ne => l != r,
                _ => {
                    return Err(EvalError::InvalidOperation {
                        op: op.as_str().to_string(),
                        left: "bool".to_string(),
                        right: "bool".to_string(),
                    })
                }
            };
            return Ok(Value::Bool(result));
        }

        Err(EvalError::InvalidOperation {
            op: op.as_str().to_string(),
            left: left.type_name().to_string(),
            right: right.type_name().to_string(),
        })
    }

    fn apply_logical(&self, left: &Value, op: BinOp, right: &Value) -> Result<Value, EvalError> {
        let (Some(l), Some(r)) = (left.to_bool(), right.to_bool()) else {
            return Err(EvalError::InvalidOperation {
                op: op.as_str().to_string(),
                left: left.type_name().to_string(),
                right: right.type_name().to_string(),
            });
        };

        let result = match op {
            BinOp::And => l && r,
            BinOp::Or => l || r,
            _ => unreachable!(),
        };

        Ok(Value::Bool(result))
    }

    fn apply_bitwise(&self, left: &Value, op: BinOp, right: &Value) -> Result<Value, EvalError> {
        let (Some(l), Some(r)) = (left.to_i128(), right.to_i128()) else {
            return Err(EvalError::InvalidOperation {
                op: op.as_str().to_string(),
                left: left.type_name().to_string(),
                right: right.type_name().to_string(),
            });
        };

        let result = match op {
            BinOp::BitAnd => l & r,
            BinOp::BitOr => l | r,
            BinOp::BitXor => l ^ r,
            BinOp::Shl => l << (r as u32),
            BinOp::Shr => l >> (r as u32),
            _ => unreachable!(),
        };

        // Return same type as operands
        Ok(match left {
            Value::I8(_) => Value::I8(result as i8),
            Value::I16(_) => Value::I16(result as i16),
            Value::I32(_) => Value::I32(result as i32),
            Value::I64(_) => Value::I64(result as i64),
            Value::I128(_) => Value::I128(result),
            Value::Isize(_) => Value::Isize(result as isize),
            Value::U8(_) => Value::U8(result as u8),
            Value::U16(_) => Value::U16(result as u16),
            Value::U32(_) => Value::U32(result as u32),
            Value::U64(_) => Value::U64(result as u64),
            Value::U128(_) => Value::U128(result as u128),
            Value::Usize(_) => Value::Usize(result as usize),
            _ => unreachable!(),
        })
    }

    fn apply_unary(&self, op: UnaryOp, value: &Value) -> Result<Value, EvalError> {
        match op {
            UnaryOp::Neg => {
                if let Some(v) = value.to_i128() {
                    Ok(match value {
                        Value::I8(_) => Value::I8((-v) as i8),
                        Value::I16(_) => Value::I16((-v) as i16),
                        Value::I32(_) => Value::I32((-v) as i32),
                        Value::I64(_) => Value::I64((-v) as i64),
                        Value::I128(_) => Value::I128(-v),
                        Value::Isize(_) => Value::Isize((-v) as isize),
                        _ => {
                            return Err(EvalError::InvalidOperation {
                                op: "-".to_string(),
                                left: value.type_name().to_string(),
                                right: "".to_string(),
                            })
                        }
                    })
                } else if let Some(v) = value.to_f64() {
                    Ok(match value {
                        Value::F32(_) => Value::F32((-v) as f32),
                        Value::F64(_) => Value::F64(-v),
                        _ => unreachable!(),
                    })
                } else {
                    Err(EvalError::InvalidOperation {
                        op: "-".to_string(),
                        left: value.type_name().to_string(),
                        right: "".to_string(),
                    })
                }
            }
            UnaryOp::Not => {
                if let Some(v) = value.to_bool() {
                    Ok(Value::Bool(!v))
                } else if let Some(v) = value.to_i128() {
                    // Bitwise not for integers
                    Ok(match value {
                        Value::I8(_) => Value::I8((!v) as i8),
                        Value::I16(_) => Value::I16((!v) as i16),
                        Value::I32(_) => Value::I32((!v) as i32),
                        Value::I64(_) => Value::I64((!v) as i64),
                        Value::I128(_) => Value::I128(!v),
                        Value::Isize(_) => Value::Isize((!v) as isize),
                        Value::U8(_) => Value::U8((!v) as u8),
                        Value::U16(_) => Value::U16((!v) as u16),
                        Value::U32(_) => Value::U32((!v) as u32),
                        Value::U64(_) => Value::U64((!v) as u64),
                        Value::U128(_) => Value::U128((!v) as u128),
                        Value::Usize(_) => Value::Usize((!v) as usize),
                        _ => unreachable!(),
                    })
                } else {
                    Err(EvalError::InvalidOperation {
                        op: "!".to_string(),
                        left: value.type_name().to_string(),
                        right: "".to_string(),
                    })
                }
            }
            UnaryOp::Deref | UnaryOp::Ref => Err(EvalError::unsupported(
                "dereference/reference operators (requires runtime integration)",
            )),
        }
    }

    fn cast_value(&self, value: &Value, ty: &str) -> Result<Value, EvalError> {
        let ty = ty.trim();

        // Get numeric value
        if let Some(v) = value.to_i128() {
            return Ok(match ty {
                "i8" => Value::I8(v as i8),
                "i16" => Value::I16(v as i16),
                "i32" => Value::I32(v as i32),
                "i64" => Value::I64(v as i64),
                "i128" => Value::I128(v),
                "isize" => Value::Isize(v as isize),
                "u8" => Value::U8(v as u8),
                "u16" => Value::U16(v as u16),
                "u32" => Value::U32(v as u32),
                "u64" => Value::U64(v as u64),
                "u128" => Value::U128(v as u128),
                "usize" => Value::Usize(v as usize),
                "f32" => Value::F32(v as f32),
                "f64" => Value::F64(v as f64),
                _ => return Err(EvalError::unsupported(format!("cast to {}", ty))),
            });
        }

        if let Some(v) = value.to_f64() {
            return Ok(match ty {
                "i8" => Value::I8(v as i8),
                "i16" => Value::I16(v as i16),
                "i32" => Value::I32(v as i32),
                "i64" => Value::I64(v as i64),
                "i128" => Value::I128(v as i128),
                "isize" => Value::Isize(v as isize),
                "u8" => Value::U8(v as u8),
                "u16" => Value::U16(v as u16),
                "u32" => Value::U32(v as u32),
                "u64" => Value::U64(v as u64),
                "u128" => Value::U128(v as u128),
                "usize" => Value::Usize(v as usize),
                "f32" => Value::F32(v as f32),
                "f64" => Value::F64(v),
                _ => return Err(EvalError::unsupported(format!("cast to {}", ty))),
            });
        }

        Err(EvalError::unsupported(format!(
            "cast from {} to {}",
            value.type_name(),
            ty
        )))
    }
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expr::parser::parse_expr;

    #[test]
    fn test_literal_eval() {
        let eval = Evaluator::new();

        let expr = parse_expr("42").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::I32(42)));

        let expr = parse_expr("true").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::Bool(true)));
    }

    #[test]
    fn test_arithmetic() {
        let eval = Evaluator::new();

        let expr = parse_expr("10 + 5").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::I32(15)));

        let expr = parse_expr("10 - 5").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::I32(5)));

        let expr = parse_expr("10 * 5").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::I32(50)));
    }

    #[test]
    fn test_comparison() {
        let eval = Evaluator::new();

        let expr = parse_expr("10 > 5").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::Bool(true)));

        let expr = parse_expr("10 == 10").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::Bool(true)));
    }

    #[test]
    fn test_variable_lookup() {
        let mut eval = Evaluator::new();
        eval.set_variable("x", Value::I32(42));

        let expr = parse_expr("x").unwrap();
        let result = eval.eval(&expr).unwrap();
        assert!(matches!(result, Value::I32(42)));
    }

    #[test]
    fn test_type_mismatch() {
        let _eval = Evaluator::new();

        // This actually parses as two i32 literals, so types match
        // We need to test with variables of different types
        let mut eval = Evaluator::new();
        eval.set_variable("a", Value::I32(10));
        eval.set_variable("b", Value::F64(3.14));

        let expr = parse_expr("a + b").unwrap();
        let result = eval.eval(&expr);
        assert!(matches!(result, Err(EvalError::InvalidOperation { .. })));
    }

    #[test]
    fn test_division_by_zero() {
        let eval = Evaluator::new();

        let expr = parse_expr("10 / 0").unwrap();
        let result = eval.eval(&expr);
        assert!(matches!(result, Err(EvalError::DivisionByZero)));
    }
}
