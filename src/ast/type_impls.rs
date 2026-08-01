//! Constructors that build [`Type`] nodes directly from lexer tokens.

use super::*;
use crate::ast::interner::Interner;
use crate::lexer::token::Token;

impl Ty {
    /// Builds the type for a primitive keyword token, such as `i32` or `bool`.
    ///
    /// The keyword's own text becomes a single-segment [`Path`], so a primitive type looks the
    /// same to later passes as a user-defined type named after that keyword.
    pub fn primitive(tok: Token) -> Ty {
        let ident = Ident {
            text: Interner::intern(tok.kind.to_string()),
            span: tok.span,
        };
        Ty {
            kind: TyKind::Base {
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
