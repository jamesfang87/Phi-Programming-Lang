use super::*;
use crate::lexer::token::Token;
use crate::session::Session;

impl Path {
    pub fn primitive(session: &Session, tok: Token) -> Path {
        Path::from(Ident::of_token(session, tok))
    }
}

impl From<Ident> for Path {
    fn from(ident: Ident) -> Self {
        Path {
            segments: vec![ident],
        }
    }
}

impl Ty {
    pub fn primitive(session: &Session, tok: Token) -> Ty {
        let path = Path::primitive(session, tok);
        let span = path.span();
        Ty {
            id: NodeId::next(),
            span,
            kind: TyKind::Path {
                path,
                args: Vec::new(),
            },
        }
    }
}
