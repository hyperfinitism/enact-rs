// SPDX-License-Identifier: Apache-2.0

use crate::error::Error;

/// Token produced by the expression lexer.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    StringLit(String),
    NumberLit(f64),
    True,
    False,
    Null,

    // Identifier
    Ident(String),

    // Operators
    Eq,   // ==
    Neq,  // !=
    Lt,   // <
    Gt,   // >
    Lte,  // <=
    Gte,  // >=
    And,  // &&
    Or,   // ||
    Bang, // !

    // Punctuation
    Dot,      // .
    Comma,    // ,
    LParen,   // (
    RParen,   // )
    LBracket, // [
    RBracket, // ]
    Star,     // * (wildcard filter)

    Eof,
}

pub struct Lexer {
    chars: Vec<char>,
    pos: usize,
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            chars: input.chars().collect(),
            pos: 0,
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, Error> {
        let mut tokens = Vec::new();
        loop {
            self.skip_whitespace();
            if self.pos >= self.chars.len() {
                tokens.push(Token::Eof);
                break;
            }
            let tok = self.next_token()?;
            tokens.push(tok);
        }
        Ok(tokens)
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.pos).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.chars.get(self.pos).copied();
        self.pos += 1;
        c
    }

    fn skip_whitespace(&mut self) {
        while self.pos < self.chars.len() && self.chars[self.pos].is_whitespace() {
            self.pos += 1;
        }
    }

    fn next_token(&mut self) -> Result<Token, Error> {
        let c = self.peek().unwrap();
        match c {
            '\'' => self.read_string(),
            '(' => {
                self.advance();
                Ok(Token::LParen)
            }
            ')' => {
                self.advance();
                Ok(Token::RParen)
            }
            '[' => {
                self.advance();
                Ok(Token::LBracket)
            }
            ']' => {
                self.advance();
                Ok(Token::RBracket)
            }
            '.' => {
                self.advance();
                Ok(Token::Dot)
            }
            ',' => {
                self.advance();
                Ok(Token::Comma)
            }
            '*' => {
                self.advance();
                Ok(Token::Star)
            }
            '!' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Neq)
                } else {
                    Ok(Token::Bang)
                }
            }
            '=' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Eq)
                } else {
                    Err(Error::ExpressionSyntax {
                        position: self.pos,
                        message: "expected '==' but got single '='".to_string(),
                    })
                }
            }
            '<' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Lte)
                } else {
                    Ok(Token::Lt)
                }
            }
            '>' => {
                self.advance();
                if self.peek() == Some('=') {
                    self.advance();
                    Ok(Token::Gte)
                } else {
                    Ok(Token::Gt)
                }
            }
            '&' => {
                self.advance();
                if self.peek() == Some('&') {
                    self.advance();
                    Ok(Token::And)
                } else {
                    Err(Error::ExpressionSyntax {
                        position: self.pos,
                        message: "expected '&&' but got single '&'".to_string(),
                    })
                }
            }
            '|' => {
                self.advance();
                if self.peek() == Some('|') {
                    self.advance();
                    Ok(Token::Or)
                } else {
                    Err(Error::ExpressionSyntax {
                        position: self.pos,
                        message: "expected '||' but got single '|'".to_string(),
                    })
                }
            }
            c if c.is_ascii_digit() || (c == '-' && self.is_start_of_negative_number()) => {
                self.read_number()
            }
            c if c.is_ascii_alphabetic() || c == '_' => self.read_ident_or_keyword(),
            _ => Err(Error::ExpressionSyntax {
                position: self.pos,
                message: format!("unexpected character: '{c}'"),
            }),
        }
    }

    fn is_start_of_negative_number(&self) -> bool {
        // `-` is a negative number only if followed by a digit
        // and not preceded by an identifier/literal (context would be needed for full check,
        // but this is sufficient for GHA expressions which have no subtraction)
        self.pos + 1 < self.chars.len() && self.chars[self.pos + 1].is_ascii_digit()
    }

    fn read_string(&mut self) -> Result<Token, Error> {
        self.advance(); // consume opening quote
        let mut s = String::new();
        loop {
            match self.advance() {
                None => {
                    return Err(Error::ExpressionSyntax {
                        position: self.pos,
                        message: "unterminated string literal".to_string(),
                    });
                }
                Some('\'') => {
                    // Doubled single quote is an escape
                    if self.peek() == Some('\'') {
                        self.advance();
                        s.push('\'');
                    } else {
                        break;
                    }
                }
                Some(c) => s.push(c),
            }
        }
        Ok(Token::StringLit(s))
    }

    fn read_number(&mut self) -> Result<Token, Error> {
        let start = self.pos;
        let mut s = String::new();
        if self.peek() == Some('-') {
            s.push('-');
            self.advance();
        }
        // Check for hex
        if self.peek() == Some('0')
            && self.pos + 1 < self.chars.len()
            && (self.chars[self.pos + 1] == 'x' || self.chars[self.pos + 1] == 'X')
        {
            s.push('0');
            self.advance();
            s.push('x');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            let hex = &s[2..]; // skip "0x"
            let val = u64::from_str_radix(hex, 16).map_err(|_| Error::ExpressionSyntax {
                position: start,
                message: format!("invalid hex number: {s}"),
            })?;
            return Ok(Token::NumberLit(val as f64));
        }
        // Decimal
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        // Fractional part
        if self.peek() == Some('.') {
            s.push('.');
            self.advance();
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        // Exponent
        if let Some('e' | 'E') = self.peek() {
            s.push('e');
            self.advance();
            if let Some('+' | '-') = self.peek() {
                s.push(self.advance().unwrap());
            }
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() {
                    s.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
        }
        let val: f64 = s.parse().map_err(|_| Error::ExpressionSyntax {
            position: start,
            message: format!("invalid number: {s}"),
        })?;
        Ok(Token::NumberLit(val))
    }

    fn read_ident_or_keyword(&mut self) -> Result<Token, Error> {
        let mut s = String::new();
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                s.push(c);
                self.advance();
            } else {
                break;
            }
        }
        match s.as_str() {
            "true" => Ok(Token::True),
            "false" => Ok(Token::False),
            "null" => Ok(Token::Null),
            _ => Ok(Token::Ident(s)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokenize(input: &str) -> Vec<Token> {
        Lexer::new(input).tokenize().unwrap()
    }

    #[test]
    fn test_simple_ident() {
        let tokens = tokenize("github.ref");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("github".into()),
                Token::Dot,
                Token::Ident("ref".into()),
                Token::Eof
            ]
        );
    }

    #[test]
    fn test_string_literal() {
        let tokens = tokenize("'hello world'");
        assert_eq!(
            tokens,
            vec![Token::StringLit("hello world".into()), Token::Eof]
        );
    }

    #[test]
    fn test_escaped_quote() {
        let tokens = tokenize("'it''s'");
        assert_eq!(tokens, vec![Token::StringLit("it's".into()), Token::Eof]);
    }

    #[test]
    fn test_comparison_operators() {
        let tokens = tokenize("a == b != c < d > e <= f >= g");
        assert!(tokens.contains(&Token::Eq));
        assert!(tokens.contains(&Token::Neq));
        assert!(tokens.contains(&Token::Lt));
        assert!(tokens.contains(&Token::Gt));
        assert!(tokens.contains(&Token::Lte));
        assert!(tokens.contains(&Token::Gte));
    }

    #[test]
    fn test_logical_operators() {
        let tokens = tokenize("a && b || !c");
        assert!(tokens.contains(&Token::And));
        assert!(tokens.contains(&Token::Or));
        assert!(tokens.contains(&Token::Bang));
    }

    #[test]
    fn test_number_literals() {
        let tokens = tokenize("42 1.2 0xFF -1 1e10");
        assert_eq!(tokens[0], Token::NumberLit(42.0));
        assert_eq!(tokens[1], Token::NumberLit(1.2));
        assert_eq!(tokens[2], Token::NumberLit(255.0));
        assert_eq!(tokens[3], Token::NumberLit(-1.0));
        assert_eq!(tokens[4], Token::NumberLit(1e10));
    }

    #[test]
    fn test_function_call() {
        let tokens = tokenize("contains(github.event, 'push')");
        assert_eq!(tokens[0], Token::Ident("contains".into()));
        assert_eq!(tokens[1], Token::LParen);
    }

    #[test]
    fn test_wildcards_and_brackets() {
        let tokens = tokenize("labels.*.name");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("labels".into()),
                Token::Dot,
                Token::Star,
                Token::Dot,
                Token::Ident("name".into()),
                Token::Eof,
            ]
        );
    }
}
