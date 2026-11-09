use super::*;
use crate::driver::source::SrcSpan;
use crate::lexer::literal;
use crate::lexer::token::Token;
use crate::session::Session;

impl Expr {
    pub fn new(kind: ExprKind, span: SrcSpan) -> Self {
        Expr {
            id: NodeId::next(),
            kind,
            span,
        }
    }

    /// Builds the `Some(..)`/`None` variant expr for one bound of a desugared range.
    pub fn range_bound_variant(session: &Session, bound: Option<Expr>, span: SrcSpan) -> Expr {
        let (name, payload) = match bound {
            Some(value) => ("some", Payload::Single(Box::new(value))),
            None => ("none", Payload::None),
        };
        Expr {
            id: NodeId::next(),
            kind: ExprKind::Variant {
                variant: Ident {
                    text: session.intern(name),
                    span,
                },
                payload,
            },
            span,
        }
    }

    pub fn binary(lhs: Expr, ((op, _op_span), rhs): ((BinaryOp, SrcSpan), Expr)) -> Self {
        let span = lhs.span.merge(rhs.span);
        Expr::new(
            ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        )
    }
}

impl Literal {
    pub fn int(session: &Session, tok: Token) -> Literal {
        let (value, suffix) = numeric_literal_parts(session, tok);
        Literal::Int {
            value: session.intern(&value),
            suffix,
        }
    }

    pub fn negated_int(session: &Session, tok: Token) -> Literal {
        let (value, suffix) = numeric_literal_parts(session, tok);
        Literal::Int {
            value: session.intern(&format!("-{value}")),
            suffix,
        }
    }

    pub fn float(session: &Session, tok: Token) -> Literal {
        let (value, suffix) = numeric_literal_parts(session, tok);
        Literal::Float {
            value: session.intern(&value),
            suffix,
        }
    }

    pub fn negated_float(session: &Session, tok: Token) -> Literal {
        let (value, suffix) = numeric_literal_parts(session, tok);
        Literal::Float {
            value: session.intern(&format!("-{value}")),
            suffix,
        }
    }

    pub fn string(session: &Session, tok: Token) -> Literal {
        let inner = quoted_inner(&tok.text(session), '"');
        Literal::Str(session.intern(&literal::decode_escapes(&inner)))
    }

    pub fn char(session: &Session, tok: Token) -> Literal {
        let inner = quoted_inner(&tok.text(session), '\'');
        let ch = literal::decode_escapes(&inner)
            .chars()
            .next()
            .unwrap_or('\0');
        Literal::Char(ch)
    }
}

fn numeric_literal_parts(session: &Session, tok: Token) -> (String, Option<Symbol>) {
    let text = tok.text(session);
    let (value, suffix) = literal::split_suffix(&text);
    let value = literal::strip_digit_separators(value);
    (value, suffix.map(|suffix| session.intern(suffix)))
}

fn quoted_inner(text: &str, quote: char) -> Vec<char> {
    text.strip_prefix(quote)
        .map(|rest| rest.strip_suffix(quote).unwrap_or(rest))
        .unwrap_or(text)
        .chars()
        .collect()
}
