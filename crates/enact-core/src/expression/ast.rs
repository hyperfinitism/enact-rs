// SPDX-License-Identifier: Apache-2.0

use super::value::Value;

/// AST node for a GitHub Actions expression.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Literal value (string, number, bool, null).
    Literal(Value),
    /// Identifier — a bare name like `github`, `env`, `true`, etc.
    Ident(String),
    /// Property access: `expr.name`
    Property(Box<Expr>, String),
    /// Index access: `expr[expr]`
    Index(Box<Expr>, Box<Expr>),
    /// Wildcard filter: `expr.*`
    Wildcard(Box<Expr>),
    /// Function call: `name(args...)`
    FunctionCall(String, Vec<Expr>),
    /// Unary NOT: `!expr`
    Not(Box<Expr>),
    /// Binary operation.
    BinaryOp(BinaryOp, Box<Expr>, Box<Expr>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Or,
    And,
    Eq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
}
