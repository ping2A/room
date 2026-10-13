//! DECORATE expression IR (`DecoExpr`) — compile-time parse, run-time eval by host.

use crate::error::DecorateError;
use ferrite_sc::{Punct, ScanOptions, Scanner, Tok};

/// Binary operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BinOp {
    /// `+`
    Add,
    /// `-`
    Sub,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Mod,
}

/// Tiny typed expression tree for action / property args.
#[derive(Clone, Debug, PartialEq)]
pub enum DecoExpr {
    /// Integer literal.
    Int(i32),
    /// Floating literal.
    Float(f64),
    /// Quoted string.
    String(String),
    /// Identifier (`health`, `user_foo`, jump label, …).
    Ident(String),
    /// Unary minus.
    Neg(Box<DecoExpr>),
    /// Binary op.
    Bin {
        /// Operator.
        op: BinOp,
        /// Left.
        lhs: Box<DecoExpr>,
        /// Right.
        rhs: Box<DecoExpr>,
    },
    /// `name(args…)`.
    Call {
        /// Function name (`random`, …).
        name: String,
        /// Arguments.
        args: Vec<DecoExpr>,
    },
    /// `base[index]` (`args[0]`).
    Index {
        /// Base expression.
        base: Box<DecoExpr>,
        /// Index expression.
        index: Box<DecoExpr>,
    },
}

/// Parse a DECORATE expression from a source fragment (one action argument).
pub fn parse_deco_expr(text: &str) -> Result<DecoExpr, DecorateError> {
    let mut sc = Scanner::from_str(
        text.trim(),
        ScanOptions {
            c_mode: true,
            semicolon_comments: false,
        },
    );
    let expr = parse_add(&mut sc)?;
    match sc.next_token()? {
        Tok::Eof => Ok(expr),
        other => Err(DecorateError::Message(format!(
            "trailing tokens in expression: {other:?}"
        ))),
    }
}

/// True when `text` is a bare literal / ident (no runtime eval needed beyond passthrough).
#[must_use]
pub fn is_trivial_arg(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return true;
    }
    // Quoted string.
    if t.starts_with('"') && t.ends_with('"') && t.len() >= 2 {
        return !t[1..t.len() - 1].contains('"');
    }
    // Integer / float (optional leading minus).
    if t.parse::<i32>().is_ok() || t.parse::<f64>().is_ok() {
        return true;
    }
    // Bare identifier (label / class / flag) — no operators or indexing.
    t.chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && t.chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
}

fn parse_add(sc: &mut Scanner<'_>) -> Result<DecoExpr, DecorateError> {
    let mut lhs = parse_mul(sc)?;
    loop {
        let op = match sc.next_token()? {
            Tok::Punct(Punct::Plus) => BinOp::Add,
            Tok::Punct(Punct::Minus) => BinOp::Sub,
            other => {
                sc.unget(other);
                return Ok(lhs);
            }
        };
        let rhs = parse_mul(sc)?;
        lhs = DecoExpr::Bin {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
}

fn parse_mul(sc: &mut Scanner<'_>) -> Result<DecoExpr, DecorateError> {
    let mut lhs = parse_unary(sc)?;
    loop {
        let op = match sc.next_token()? {
            Tok::Punct(Punct::Star) => BinOp::Mul,
            Tok::Punct(Punct::Slash) => BinOp::Div,
            Tok::Punct(Punct::Percent) => BinOp::Mod,
            other => {
                sc.unget(other);
                return Ok(lhs);
            }
        };
        let rhs = parse_unary(sc)?;
        lhs = DecoExpr::Bin {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        };
    }
}

fn parse_unary(sc: &mut Scanner<'_>) -> Result<DecoExpr, DecorateError> {
    match sc.next_token()? {
        Tok::Punct(Punct::Minus) => Ok(DecoExpr::Neg(Box::new(parse_unary(sc)?))),
        Tok::Punct(Punct::Plus) => parse_unary(sc),
        other => {
            sc.unget(other);
            parse_postfix(sc)
        }
    }
}

fn parse_postfix(sc: &mut Scanner<'_>) -> Result<DecoExpr, DecorateError> {
    let mut expr = parse_primary(sc)?;
    loop {
        match sc.next_token()? {
            Tok::Punct(Punct::LBracket) => {
                let index = parse_add(sc)?;
                match sc.next_token()? {
                    Tok::Punct(Punct::RBracket) => {}
                    _ => {
                        return Err(DecorateError::Message("expected ']' after index".into()));
                    }
                }
                expr = DecoExpr::Index {
                    base: Box::new(expr),
                    index: Box::new(index),
                };
            }
            other => {
                sc.unget(other);
                return Ok(expr);
            }
        }
    }
}

fn parse_primary(sc: &mut Scanner<'_>) -> Result<DecoExpr, DecorateError> {
    match sc.next_token()? {
        Tok::Int(n) => Ok(DecoExpr::Int(n as i32)),
        Tok::Float(f) => Ok(DecoExpr::Float(f)),
        Tok::String(s) => Ok(DecoExpr::String(s.into_owned())),
        Tok::Ident(name) => {
            let name = name.to_owned();
            if sc.check_punct(Punct::LParen)? {
                let mut args = Vec::new();
                if !sc.check_punct(Punct::RParen)? {
                    loop {
                        args.push(parse_add(sc)?);
                        match sc.next_token()? {
                            Tok::Punct(Punct::Comma) => continue,
                            Tok::Punct(Punct::RParen) => break,
                            other => {
                                return Err(DecorateError::Message(format!(
                                    "expected ',' or ')' in call, got {other:?}"
                                )));
                            }
                        }
                    }
                }
                Ok(DecoExpr::Call { name, args })
            } else {
                Ok(DecoExpr::Ident(name))
            }
        }
        Tok::Punct(Punct::LParen) => {
            let inner = parse_add(sc)?;
            match sc.next_token()? {
                Tok::Punct(Punct::RParen) => Ok(inner),
                _ => Err(DecorateError::Message(
                    "expected ')' after expression".into(),
                )),
            }
        }
        other => Err(DecorateError::Message(format!(
            "expected expression, got {other:?}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn parses_arith_and_random() {
        let e = parse_deco_expr("random(1, 8) * 3 + 1").unwrap();
        match e {
            DecoExpr::Bin { op: BinOp::Add, .. } => {}
            other => panic!("unexpected {other:?}"),
        }
        assert!(is_trivial_arg("42"));
        assert!(is_trivial_arg("See"));
        assert!(!is_trivial_arg("health + 1"));
        assert!(!is_trivial_arg("args[0]"));
    }

    #[test]
    fn parses_args_index_and_user() {
        let e = parse_deco_expr("args[0]").unwrap();
        match e {
            DecoExpr::Index { base, .. } => match *base {
                DecoExpr::Ident(n) => assert_eq!(n, "args"),
                other => panic!("base {other:?}"),
            },
            other => panic!("{other:?}"),
        }
        let u = parse_deco_expr("user_count + 2").unwrap();
        assert!(matches!(u, DecoExpr::Bin { op: BinOp::Add, .. }));
    }
}
