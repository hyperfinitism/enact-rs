// SPDX-License-Identifier: Apache-2.0

use super::ast::{BinaryOp, Expr};
use super::lexer::Token;
use super::value::Value;
use crate::error::Error;

/// Recursive-descent parser for GitHub Actions expressions.
///
/// Precedence (lowest to highest):
/// 1. ||
/// 2. &&
/// 3. ==, !=
/// 4. <, >, <=, >=
/// 5. ! (unary)
/// 6. ., [], (), * (postfix)
/// 7. Atoms: literals, identifiers, parenthesized
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    pub fn parse(&mut self) -> Result<Expr, Error> {
        let expr = self.parse_or()?;
        if !self.is_at_end() {
            return Err(self.err("unexpected token after expression"));
        }
        Ok(expr)
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> &Token {
        let tok = self.tokens.get(self.pos).unwrap_or(&Token::Eof);
        self.pos += 1;
        tok
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek(), Token::Eof)
    }

    fn expect(&mut self, expected: &Token) -> Result<(), Error> {
        if self.peek() == expected {
            self.advance();
            Ok(())
        } else {
            Err(self.err(&format!("expected {expected:?}, got {:?}", self.peek())))
        }
    }

    fn err(&self, msg: &str) -> Error {
        Error::ExpressionSyntax {
            position: self.pos,
            message: msg.to_string(),
        }
    }

    // --- Precedence levels ---

    fn parse_or(&mut self) -> Result<Expr, Error> {
        let mut left = self.parse_and()?;
        while matches!(self.peek(), Token::Or) {
            self.advance();
            let right = self.parse_and()?;
            left = Expr::BinaryOp(BinaryOp::Or, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<Expr, Error> {
        let mut left = self.parse_equality()?;
        while matches!(self.peek(), Token::And) {
            self.advance();
            let right = self.parse_equality()?;
            left = Expr::BinaryOp(BinaryOp::And, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<Expr, Error> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = match self.peek() {
                Token::Eq => BinaryOp::Eq,
                Token::Neq => BinaryOp::Neq,
                _ => break,
            };
            self.advance();
            let right = self.parse_comparison()?;
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<Expr, Error> {
        let mut left = self.parse_unary()?;
        loop {
            let op = match self.peek() {
                Token::Lt => BinaryOp::Lt,
                Token::Gt => BinaryOp::Gt,
                Token::Lte => BinaryOp::Lte,
                Token::Gte => BinaryOp::Gte,
                _ => break,
            };
            self.advance();
            let right = self.parse_unary()?;
            left = Expr::BinaryOp(op, Box::new(left), Box::new(right));
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<Expr, Error> {
        if matches!(self.peek(), Token::Bang) {
            self.advance();
            let expr = self.parse_unary()?;
            return Ok(Expr::Not(Box::new(expr)));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<Expr, Error> {
        let mut expr = self.parse_atom()?;
        loop {
            match self.peek() {
                Token::Dot => {
                    self.advance();
                    if matches!(self.peek(), Token::Star) {
                        self.advance();
                        expr = Expr::Wildcard(Box::new(expr));
                    } else if let Token::Ident(name) = self.peek().clone() {
                        self.advance();
                        expr = Expr::Property(Box::new(expr), name);
                    } else if let Token::NumberLit(_) = self.peek() {
                        // Handle things like steps.0 (numeric property)
                        let Token::NumberLit(n) = self.advance().clone() else {
                            unreachable!()
                        };
                        expr = Expr::Property(Box::new(expr), format_index(n));
                    } else {
                        return Err(self.err("expected property name after '.'"));
                    }
                }
                Token::LBracket => {
                    self.advance();
                    let index = self.parse_or()?;
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Index(Box::new(expr), Box::new(index));
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    fn parse_atom(&mut self) -> Result<Expr, Error> {
        match self.peek().clone() {
            Token::StringLit(s) => {
                self.advance();
                Ok(Expr::Literal(Value::String(s)))
            }
            Token::NumberLit(n) => {
                self.advance();
                Ok(Expr::Literal(Value::Number(n)))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Literal(Value::Bool(true)))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Literal(Value::Bool(false)))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Literal(Value::Null))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_or()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::Ident(name) => {
                self.advance();
                // Check if this is a function call
                if matches!(self.peek(), Token::LParen) {
                    self.advance(); // consume (
                    let mut args = Vec::new();
                    if !matches!(self.peek(), Token::RParen) {
                        args.push(self.parse_or()?);
                        while matches!(self.peek(), Token::Comma) {
                            self.advance();
                            args.push(self.parse_or()?);
                        }
                    }
                    self.expect(&Token::RParen)?;
                    Ok(Expr::FunctionCall(name, args))
                } else {
                    Ok(Expr::Ident(name))
                }
            }
            _ => Err(self.err(&format!("unexpected token: {:?}", self.peek()))),
        }
    }
}

fn format_index(n: f64) -> String {
    if n == n.trunc() && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::expression::lexer::Lexer;

    fn parse(input: &str) -> Expr {
        let tokens = Lexer::new(input).tokenize().unwrap();
        Parser::new(tokens).parse().unwrap()
    }

    #[test]
    fn test_literal() {
        match parse("'hello'") {
            Expr::Literal(Value::String(s)) => assert_eq!(s, "hello"),
            other => panic!("expected string literal, got {other:?}"),
        }
    }

    #[test]
    fn test_property_chain() {
        let expr = parse("github.event.pull_request.head.ref");
        // Should be nested Property accesses
        match &expr {
            Expr::Property(_, name) => assert_eq!(name, "ref"),
            other => panic!("expected property, got {other:?}"),
        }
    }

    #[test]
    fn test_function_call() {
        let expr = parse("contains(github.event, 'push')");
        match expr {
            Expr::FunctionCall(name, args) => {
                assert_eq!(name, "contains");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected function call, got {other:?}"),
        }
    }

    #[test]
    fn test_binary_ops() {
        let expr = parse("a == 'b' && c != 'd'");
        match expr {
            Expr::BinaryOp(BinaryOp::And, _, _) => {}
            other => panic!("expected AND, got {other:?}"),
        }
    }

    #[test]
    fn test_negation() {
        let expr = parse("!failure()");
        match expr {
            Expr::Not(inner) => match *inner {
                Expr::FunctionCall(name, _) => assert_eq!(name, "failure"),
                other => panic!("expected function call, got {other:?}"),
            },
            other => panic!("expected Not, got {other:?}"),
        }
    }

    #[test]
    fn test_wildcard() {
        let expr = parse("github.event.issue.labels.*.name");
        // labels.*.name => Property(Wildcard(Property(..., "labels")), "name")
        match expr {
            Expr::Property(inner, name) => {
                assert_eq!(name, "name");
                match *inner {
                    Expr::Wildcard(_) => {}
                    other => panic!("expected wildcard, got {other:?}"),
                }
            }
            other => panic!("expected property, got {other:?}"),
        }
    }

    #[test]
    fn test_index_access() {
        let expr = parse("matrix['os']");
        match expr {
            Expr::Index(_, _) => {}
            other => panic!("expected index, got {other:?}"),
        }
    }

    #[test]
    fn test_precedence() {
        // || has lower precedence than &&
        let expr = parse("a || b && c");
        match expr {
            Expr::BinaryOp(BinaryOp::Or, _, right) => match *right {
                Expr::BinaryOp(BinaryOp::And, _, _) => {}
                other => panic!("expected AND on right, got {other:?}"),
            },
            other => panic!("expected OR, got {other:?}"),
        }
    }

    #[test]
    fn test_parenthesized() {
        let expr = parse("(a || b) && c");
        match expr {
            Expr::BinaryOp(BinaryOp::And, left, _) => match *left {
                Expr::BinaryOp(BinaryOp::Or, _, _) => {}
                other => panic!("expected OR on left, got {other:?}"),
            },
            other => panic!("expected AND, got {other:?}"),
        }
    }
}
