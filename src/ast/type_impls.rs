use super::*;
use crate::ast::interner::Interner;
use crate::lexer::token::Token;

impl Path {
    pub fn primitive(tok: Token) -> Path {
        let ident = Ident {
            text: Interner::intern(tok.kind.to_string()),
            span: tok.span,
        };
        Path {
            segments: vec![ident],
            span: tok.span,
        }
    }
}

impl Ty {
    pub fn primitive(tok: Token) -> Ty {
        let path = Path::primitive(tok);
        Ty {
            id: NodeId::next(),
            span: path.span,
            kind: TyKind::Path {
                path,
                args: Vec::new(),
            },
        }
    }
}
