use super::*;
use crate::ast::interner::Interner;
use crate::lexer::token::Token;

impl Type {
    pub fn primitive(tok: Token) -> Type {
        let ident = Ident {
            text: Interner::intern(tok.kind.to_string()),
            span: tok.span,
        };
        Type {
            kind: Ty::Base {
                base: Path {
                    segments: vec![ident],
                    span: tok.span,
                },
                args: Vec::new(),
            },
            span: tok.span,
        }
    }
}
