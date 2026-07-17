// ast/expr_impls.rs
use super::*;
use crate::ast::interner::Interner;
use crate::driver::src_map::SrcMap;
use crate::lexer::{src_span::SrcSpan, token::Token, unescape};

impl Expr {
    pub fn new(kind: ExprKind, span: SrcSpan) -> Self {
        Expr { kind, span }
    }

    pub fn int(value_tok: Token, suffix_tok: Token) -> Expr {
        let value_text = SrcMap::text_of(value_tok.span)
            .expect("lexer token span should always resolve to a source file");
        let suffix_text = SrcMap::text_of(suffix_tok.span)
            .expect("lexer token span should always resolve to a source file");

        Expr {
            kind: ExprKind::Literal(Literal::Int {
                value: Interner::intern(&value_text),
                suffix: Interner::intern(&suffix_text),
            }),
            span: value_tok.span.merge(suffix_tok.span),
        }
    }

    pub fn float(value_tok: Token, suffix_tok: Token) -> Expr {
        let value_text = SrcMap::text_of(value_tok.span)
            .expect("lexer token span should always resolve to a source file");
        let suffix_text = SrcMap::text_of(suffix_tok.span)
            .expect("lexer token span should always resolve to a source file");

        Expr {
            kind: ExprKind::Literal(Literal::Float {
                value: Interner::intern(&value_text),
                suffix: Interner::intern(&suffix_text),
            }),
            span: value_tok.span.merge(suffix_tok.span),
        }
    }

    pub fn string(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        Expr {
            kind: ExprKind::Literal(Literal::Str(Interner::intern(&unescape(&inner)))),
            span: tok.span,
        }
    }

    pub fn char(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        let ch = unescape(&inner).chars().next().unwrap_or('\0');
        Expr {
            kind: ExprKind::Literal(Literal::Char(ch)),
            span: tok.span,
        }
    }

    pub fn binary(op: BinaryOp, lhs: Expr, rhs: Expr, span: SrcSpan) -> Self {
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
