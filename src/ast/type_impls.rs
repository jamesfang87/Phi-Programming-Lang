use super::*;
use crate::lexer::token::Token;

impl Path {
    pub fn primitive(tok: Token) -> Path {
        Path::from(Ident::of_token(tok))
    }
}

impl From<Ident> for Path {
    fn from(ident: Ident) -> Self {
        Path {
            segments: vec![ident],
            span: ident.span,
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
