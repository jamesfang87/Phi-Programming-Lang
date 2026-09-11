use super::*;
use crate::ast::interner::Interner;
use crate::driver::source::SrcSpan;
use crate::lexer::literal;
use crate::lexer::token::Token;

impl Expr {
    pub fn new(kind: ExprKind, span: SrcSpan) -> Self {
        Expr {
            id: NodeId::next(),
            kind,
            span,
        }
    }

    pub fn int(tok: Token) -> Expr {
        let text = tok.text();
        let (value, suffix) = literal::split_suffix(&text);
        let value = literal::strip_digit_separators(value);

        Expr::new(
            ExprKind::Literal(Literal::Int {
                value: Interner::intern(&value),
                suffix: suffix.map(Interner::intern),
            }),
            tok.span,
        )
    }

    pub fn float(tok: Token) -> Expr {
        let text = tok.text();
        let (value, suffix) = literal::split_suffix(&text);
        let value = literal::strip_digit_separators(value);

        Expr::new(
            ExprKind::Literal(Literal::Float {
                value: Interner::intern(&value),
                suffix: suffix.map(Interner::intern),
            }),
            tok.span,
        )
    }

    pub fn string(tok: Token) -> Expr {
        let chars = tok.text();
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        Expr::new(
            ExprKind::Literal(Literal::Str(Interner::intern(&literal::decode_escapes(
                &inner,
            )))),
            tok.span,
        )
    }

    pub fn char(tok: Token) -> Expr {
        let chars = tok.text();
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        let ch = literal::decode_escapes(&inner)
            .chars()
            .next()
            .unwrap_or('\0');
        Expr::new(ExprKind::Literal(Literal::Char(ch)), tok.span)
    }

    pub fn bool_literal(value: bool) -> impl Fn(Token) -> Expr + Clone {
        move |t: Token| Expr::new(ExprKind::Literal(Literal::Bool(value)), t.span)
    }

    /// Builds the `Some(..)`/`None` variant expr for one bound of a desugared range.
    pub fn range_bound_variant(bound: Option<Expr>, span: SrcSpan) -> Expr {
        let (name, payload) = match bound {
            Some(value) => ("some", Payload::Single(Box::new(value))),
            None => ("none", Payload::None),
        };
        Expr {
            id: NodeId::next(),
            kind: ExprKind::Variant {
                variant: Ident {
                    text: Interner::intern(name),
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
