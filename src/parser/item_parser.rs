use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::Enum;
use crate::ast::Field;
use crate::ast::Function;
use crate::ast::Param;
use crate::ast::Struct;
use crate::ast::Variant;
use crate::ast::VariantPayload;
use crate::ast::{Generic, Item, ItemKind, Visibility};

use crate::lexer::token::TokenKind;

use super::{BoxedP, Parser};

impl Parser {
    pub fn item_parser<'a>(&'a self) -> BoxedP<'a, Item> {
        let ident = self.ident_parser();
        let type_p = self.type_parser();
        let block = self.block_parser();

        let visibility = self
            .kind(TokenKind::PublicKw)
            .or_not()
            .map(|opt| {
                if opt.is_some() {
                    Visibility::Public
                } else {
                    Visibility::Private
                }
            })
            .boxed();

        let generic = ident
            .clone()
            .then(
                self.kind(TokenKind::Colon)
                    .ignore_then(
                        self.path_parser()
                            .clone()
                            .separated_by(self.kind(TokenKind::Plus))
                            .at_least(1)
                            .collect::<Vec<_>>(),
                    )
                    .or_not(),
            )
            .map(|(name, bounds)| {
                let span = if bounds.is_none() {
                    name.span
                } else {
                    name.span.merge(
                        bounds
                            .clone()
                            .unwrap()
                            .last()
                            .expect("there should at least one bound if bounds is not none")
                            .span,
                    )
                };

                Generic { name, bounds, span }
            })
            .boxed();

        let generics = generic
            .separated_by(self.kind(TokenKind::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        let param = ident
            .clone()
            .then_ignore(self.kind(TokenKind::Colon))
            .then(type_p.clone())
            .map(|(name, ty)| {
                let span = name.span.merge(ty.span);
                Param { name, ty, span }
            })
            .boxed();

        let params = param
            .separated_by(self.kind(TokenKind::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        let ret_ty = self
            .kind(TokenKind::Arrow)
            .ignore_then(type_p.clone())
            .or_not();

        let function_decl = visibility
            .clone()
            .then(self.kind(TokenKind::FunKw))
            .then(ident.clone())
            .then(
                self.kind(TokenKind::OpenCaret) // <
                    .ignore_then(generics.clone())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then_ignore(self.kind(TokenKind::OpenParen))
            .then(params)
            .then_ignore(self.kind(TokenKind::CloseParen))
            .then(ret_ty)
            .then(block)
            .map(
                |((((((vis, fun_tok), name), generics), params), ret), body)| {
                    let span = fun_tok.span.merge(body.span);
                    let fun = Function {
                        visibility: vis,
                        name,
                        generics: generics.unwrap_or(Vec::new()),
                        self_param: None,
                        params,
                        ret,
                        body: Some(body),
                        span,
                    };
                    Item {
                        kind: ItemKind::Function(fun),
                        span,
                    }
                },
            )
            .boxed();

        let field = self
            .kind(TokenKind::PublicKw)
            .or_not()
            .then(ident.clone())
            .then_ignore(self.kind(TokenKind::Colon))
            .then(type_p.clone())
            .map(|((pub_tok, name), ty)| {
                let span = name.span.merge(ty.span);

                let visibility = if pub_tok.is_some() {
                    Visibility::Public
                } else {
                    Visibility::Private
                };

                Field {
                    visibility,
                    name,
                    ty,
                    span,
                }
            })
            .boxed();

        let fields = field
            .clone()
            .separated_by(self.kind(TokenKind::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        let struct_decl = visibility
            .clone()
            .then(self.kind(TokenKind::StructKw))
            .then(ident.clone())
            .then(
                self.kind(TokenKind::OpenCaret) // <
                    .ignore_then(generics.clone())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(fields.clone())
            .then_ignore(self.kind(TokenKind::CloseBrace))
            .map(|((((visibility, struct_tok), name), generics), fields)| {
                let end_span = if !fields.is_empty() {
                    fields.last().unwrap().span
                } else {
                    name.span
                };
                let span = struct_tok.span.merge(end_span);

                let struct_ = Struct {
                    visibility,
                    name,
                    generics,
                    fields,
                    span,
                };

                Item {
                    kind: ItemKind::Struct(struct_),
                    span,
                }
            })
            .boxed();

        let regular_payload = ident
            .clone()
            .then(
                self.kind(TokenKind::Colon)
                    .ignore_then(type_p.clone())
                    .or_not(),
            )
            .map(|(name, ty)| {
                if ty.is_none() {
                    return Variant {
                        name,
                        payload: VariantPayload::Unit,
                        span: name.span,
                    };
                }

                let span = name.span.merge(ty.clone().unwrap().span);
                Variant {
                    name,
                    payload: VariantPayload::Type(ty.unwrap()),
                    span,
                }
            })
            .boxed();

        let record_payload = ident
            .clone()
            .then_ignore(self.kind(TokenKind::Colon))
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(
                field
                    .clone()
                    .separated_by(self.kind(TokenKind::Comma))
                    .allow_trailing()
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then_ignore(self.kind(TokenKind::CloseBrace))
            .map(|(name, fields)| {
                let span = name.span.merge(
                    fields
                        .last()
                        .expect("must have at least one field in a record payload")
                        .span,
                );
                Variant {
                    name,
                    payload: VariantPayload::Record(fields),
                    span,
                }
            })
            .boxed();

        let variant = choice((record_payload, regular_payload))
            .separated_by(self.kind(TokenKind::Comma))
            .allow_trailing()
            .collect::<Vec<_>>();

        let enum_decl = visibility
            .then(self.kind(TokenKind::StructKw))
            .then(ident.clone())
            .then(
                self.kind(TokenKind::OpenCaret) // <
                    .ignore_then(generics)
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(variant)
            .then_ignore(self.kind(TokenKind::CloseBrace))
            .map(|((((visibility, struct_tok), name), generics), variants)| {
                let end_span = if !variants.is_empty() {
                    variants.last().unwrap().span
                } else {
                    name.span
                };
                let span = struct_tok.span.merge(end_span);

                let enum_ = Enum {
                    visibility,
                    name,
                    generics,
                    variants,
                    span,
                };

                Item {
                    kind: ItemKind::Enum(enum_),
                    span,
                }
            })
            .boxed();

        choice((function_decl, struct_decl, enum_decl)).boxed()
    }
}
