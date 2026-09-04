use super::*;
use crate::ast::interner::Interner;
use crate::lexer::token::Token;

impl Ty {
    pub fn primitive(tok: Token) -> Ty {
        let ident = Ident {
            text: Interner::intern(tok.kind.to_string()),
            span: tok.span,
        };
        Ty {
            id: NodeId::next(),
            kind: TyKind::Path {
                path: Path {
                    segments: vec![ident],
                    span: tok.span,
                },
                args: Vec::new(),
            },
            span: tok.span,
        }
    }
}
