use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{Path, Ty, Type};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Parser};

impl Parser {
    pub fn type_parser<'a>(&'a self) -> BoxedP<'a, Type> {
        let primitive_ty = choice((
            self.kind(TokenKind::I8),
            self.kind(TokenKind::I16),
            self.kind(TokenKind::I32),
            self.kind(TokenKind::I64),
            self.kind(TokenKind::U8),
            self.kind(TokenKind::U16),
            self.kind(TokenKind::U32),
            self.kind(TokenKind::U64),
            self.kind(TokenKind::F32),
            self.kind(TokenKind::F64),
            self.kind(TokenKind::BoolKw),
            self.kind(TokenKind::Char),
            self.kind(TokenKind::String),
        ))
        .map(|t: Token| Type::primitive(t))
        .boxed();

        let path_ty = self.path_parser().map(|p: Path| Type {
            span: p.span,
            kind: Ty::Base {
                base: p,
                args: Vec::new(),
            },
        });

        choice((primitive_ty, path_ty)).boxed()
    }
}
