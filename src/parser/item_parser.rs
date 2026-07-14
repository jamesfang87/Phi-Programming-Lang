use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::Ident;
use crate::ast::Import;
use crate::ast::ModuleDecl;
use crate::ast::{
    Enum, Extend, Field, Function, Generic, Item, ItemKind, Param, SelfMode, SelfParam, Struct,
    Trait, Variant, VariantPayload, Visibility,
};

use crate::lexer::src_span::SrcSpan;
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

        // `self`, `&self`, `&mut self`, `any self`.
        let self_param = choice((
            self.kind(TokenKind::Amp)
                .then(self.kind(TokenKind::MutKw))
                .then(self.kind(TokenKind::LowerSelfKw))
                .map(|((amp_tok, _mut_tok), self_tok)| SelfParam {
                    mode: SelfMode::Mutable,
                    span: amp_tok.span.merge(self_tok.span),
                }),
            self.kind(TokenKind::Amp)
                .then(self.kind(TokenKind::LowerSelfKw))
                .map(|(amp_tok, self_tok)| SelfParam {
                    mode: SelfMode::Immutable,
                    span: amp_tok.span.merge(self_tok.span),
                }),
            self.kind(TokenKind::AnyKw)
                .then(self.kind(TokenKind::LowerSelfKw))
                .map(|(any_tok, self_tok)| SelfParam {
                    mode: SelfMode::Any,
                    span: any_tok.span.merge(self_tok.span),
                }),
            self.kind(TokenKind::LowerSelfKw).map(|self_tok| SelfParam {
                mode: SelfMode::Move,
                span: self_tok.span,
            }),
        ))
        .boxed();

        let ret_ty = self
            .kind(TokenKind::Arrow)
            .ignore_then(type_p.clone())
            .or_not();

        let function_decl = |allow_no_impl: bool| {
            visibility
                .clone()
                .then(self.kind(TokenKind::FunKw))
                .then(ident.clone())
                .then(
                    self.kind(TokenKind::OpenCaret)
                        .ignore_then(generics.clone())
                        .then_ignore(self.kind(TokenKind::CloseCaret))
                        .or_not(),
                )
                .then_ignore(self.kind(TokenKind::OpenParen))
                .then(
                    self_param
                        .clone()
                        .then_ignore(self.kind(TokenKind::Comma).or_not())
                        .or_not(),
                )
                .then(params.clone())
                .then_ignore(self.kind(TokenKind::CloseParen))
                .then(ret_ty.clone())
                .then(if allow_no_impl {
                    choice((
                        block.clone().map(Some),
                        self.kind(TokenKind::Semicolon).to(None),
                    ))
                    .boxed()
                } else {
                    block.clone().map(Some).boxed()
                })
                .map(
                    |(((((((vis, fun_tok), name), generics), self_param), params), ret), body)| {
                        let span = match &body {
                            Some(b) => fun_tok.span.merge(b.span),
                            None => fun_tok.span.merge(name.span),
                        };

                        Function {
                            visibility: vis,
                            name,
                            generics: generics.unwrap_or_default(),
                            self_param,
                            params,
                            ret,
                            body,
                            span,
                        }
                    },
                )
                .boxed() // BoxedP<'a, Function> now, not Item
        };

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
            .clone()
            .then(self.kind(TokenKind::EnumKw))
            .then(ident.clone())
            .then(
                self.kind(TokenKind::OpenCaret) // <
                    .ignore_then(generics.clone())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(variant)
            .then_ignore(self.kind(TokenKind::CloseBrace))
            .map(|((((visibility, enum_tok), name), generics), variants)| {
                let end_span = if !variants.is_empty() {
                    variants.last().unwrap().span
                } else {
                    name.span
                };
                let span = enum_tok.span.merge(end_span);

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

        let trait_decl = visibility
            .clone()
            .then(self.kind(TokenKind::TraitKw))
            .then(ident.clone())
            .then(
                self.kind(TokenKind::OpenCaret) // <
                    .ignore_then(generics.clone())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(function_decl.clone()(true).repeated().collect::<Vec<_>>())
            .then_ignore(self.kind(TokenKind::CloseBrace))
            .map(|((((visibility, trait_tok), name), generics), functions)| {
                let end_span = if !functions.is_empty() {
                    functions.last().unwrap().span
                } else {
                    name.span
                };
                let span = trait_tok.span.merge(end_span);

                let trait_ = Trait {
                    visibility,
                    name,
                    generics,
                    functions,
                    span,
                };

                Item {
                    kind: ItemKind::Trait(trait_),
                    span,
                }
            });

        let extend = self
            .kind(TokenKind::ExtendKw)
            .then(
                self.kind(TokenKind::OpenCaret)
                    .ignore_then(self.type_parser().repeated().at_least(1).collect())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then(self.path_parser())
            .then(
                self.kind(TokenKind::OpenCaret)
                    .ignore_then(self.type_parser().repeated().at_least(1).collect())
                    .then_ignore(self.kind(TokenKind::CloseCaret))
                    .or_not(),
            )
            .then(
                self.kind(TokenKind::WithKw)
                    .ignore_then(self.path_parser())
                    .or_not()
                    .then(
                        self.kind(TokenKind::OpenCaret)
                            .ignore_then(self.type_parser().repeated().at_least(1).collect())
                            .then_ignore(self.kind(TokenKind::CloseCaret))
                            .or_not(),
                    ),
            )
            .then_ignore(self.kind(TokenKind::OpenBrace))
            .then(function_decl.clone()(false).repeated().collect())
            .then(self.kind(TokenKind::CloseBrace))
            .map(
                |(
                    (
                        (
                            (((extend_tok, extend_generics), adt_path), adt_generics),
                            (trait_path, trait_generics),
                        ),
                        methods,
                    ),
                    close_tok,
                )| {
                    let span = extend_tok.span.merge(close_tok.span);
                    let extend = Extend {
                        extend_generics,
                        adt_generics,
                        trait_generics,
                        adt_path,
                        trait_path,
                        methods,
                        span,
                    };

                    Item {
                        kind: ItemKind::Extend(extend),
                        span,
                    }
                },
            );

        let module_decl = self
            .kind(TokenKind::ModuleKw)
            .then(self.path_parser().clone())
            .then(self.kind(TokenKind::Semicolon))
            .map(|((mod_tok, path), semi_tok)| {
                let span = mod_tok.span.merge(semi_tok.span);
                let module = ModuleDecl { path, span };

                Item {
                    kind: ItemKind::Module(module),
                    span,
                }
            })
            .boxed();

        let glob_suffix = self
            .kind(TokenKind::DoubleColon)
            .then(self.kind(TokenKind::Star))
            .map(|(_colon_tok, star_tok)| star_tok.span);

        let alias_suffix = self.kind(TokenKind::AsKw).ignore_then(ident.clone());

        enum ImportSuffix {
            Glob(SrcSpan),
            Alias(Ident),
        }

        let suffix = choice((
            glob_suffix.map(ImportSuffix::Glob),
            alias_suffix.map(ImportSuffix::Alias),
        ))
        .or_not();

        let import = self
            .kind(TokenKind::ImportKw)
            .then(self.path_parser().clone())
            .then(suffix)
            .then(self.kind(TokenKind::Semicolon))
            .map(|(((import_tok, path), suffix), semi_tok)| {
                let span = import_tok.span.merge(semi_tok.span);
                let (alias, glob) = match suffix {
                    Some(ImportSuffix::Glob(_)) => (None, true),
                    Some(ImportSuffix::Alias(name)) => (Some(name), false),
                    None => (None, false),
                };
                let import = Import {
                    path,
                    alias,
                    glob,
                    span,
                };

                Item {
                    kind: ItemKind::Import(import),
                    span,
                }
            })
            .boxed();

        choice((
            function_decl.clone()(false).map(|fun: Function| {
                let span = fun.span;
                Item {
                    kind: ItemKind::Function(fun),
                    span,
                }
            }),
            struct_decl,
            enum_decl,
            trait_decl,
            extend,
            module_decl,
            import,
        ))
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::SelfMode;
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::interner::Interner;
    use crate::lexer::Lexer;

    fn parse_item(src: &str) -> Item {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
        let (output, errors) = parser
            .item_parser()
            .parse(&tokens[..])
            .into_output_errors();
        assert!(
            errors.is_empty(),
            "unexpected parse errors for {src:?}: {errors:?}"
        );
        output.expect("expected a successfully parsed item")
    }

    fn text(ident: Ident) -> String {
        Interner::resolve(ident.text)
    }

    fn as_function(item: &Item) -> &Function {
        match &item.kind {
            ItemKind::Function(f) => f,
            other => panic!("expected a function item, got {other:?}"),
        }
    }

    #[test]
    fn parses_plain_function() {
        let item = parse_item("fun foo() {}");
        let f = as_function(&item);
        assert_eq!(text(f.name), "foo");
        assert!(f.generics.is_empty());
        assert!(f.self_param.is_none());
        assert!(f.params.is_empty());
        assert!(f.ret.is_none());
    }

    #[test]
    fn parses_function_with_generics_and_return_type() {
        let item = parse_item("fun make<T: Default>() -> T {}");
        let f = as_function(&item);
        assert_eq!(f.generics.len(), 1);
        assert_eq!(text(f.generics[0].name), "T");
        let bounds = f.generics[0].bounds.as_ref().expect("expected bounds");
        assert_eq!(text(bounds[0].segments[0]), "Default");
        assert!(f.ret.is_some());
    }

    #[test]
    fn parses_function_with_move_self() {
        let item = parse_item("fun consume(self) {}");
        let f = as_function(&item);
        let self_param = f.self_param.as_ref().expect("expected a self param");
        assert!(matches!(self_param.mode, SelfMode::Move));
        assert!(f.params.is_empty());
    }

    #[test]
    fn parses_function_with_immutable_ref_self() {
        let item = parse_item("fun get(&self) {}");
        let f = as_function(&item);
        let self_param = f.self_param.as_ref().expect("expected a self param");
        assert!(matches!(self_param.mode, SelfMode::Immutable));
    }

    #[test]
    fn parses_function_with_mutable_ref_self() {
        let item = parse_item("fun set(&mut self) {}");
        let f = as_function(&item);
        let self_param = f.self_param.as_ref().expect("expected a self param");
        assert!(matches!(self_param.mode, SelfMode::Mutable));
    }

    #[test]
    fn parses_function_with_any_self() {
        let item = parse_item("fun run(any self) {}");
        let f = as_function(&item);
        let self_param = f.self_param.as_ref().expect("expected a self param");
        assert!(matches!(self_param.mode, SelfMode::Any));
    }

    #[test]
    fn parses_function_with_self_and_params() {
        let item = parse_item("fun add(&mut self, x: i32, y: i32) {}");
        let f = as_function(&item);
        assert!(matches!(
            f.self_param.as_ref().unwrap().mode,
            SelfMode::Mutable
        ));
        assert_eq!(f.params.len(), 2);
        assert_eq!(text(f.params[0].name), "x");
        assert_eq!(text(f.params[1].name), "y");
    }

    #[test]
    fn parses_struct_with_generics_and_mixed_visibility_fields() {
        let item = parse_item("struct Point<T: Bounded> { x: T, public y: T }");
        match &item.kind {
            ItemKind::Struct(s) => {
                assert_eq!(text(s.name), "Point");
                let generics = s.generics.as_ref().expect("expected generics");
                assert_eq!(generics.len(), 1);
                assert_eq!(text(generics[0].name), "T");
                assert_eq!(s.fields.len(), 2);
                assert!(matches!(s.fields[0].visibility, Visibility::Private));
                assert!(matches!(s.fields[1].visibility, Visibility::Public));
            }
            other => panic!("expected a struct item, got {other:?}"),
        }
    }

    #[test]
    fn parses_struct_without_generics() {
        let item = parse_item("struct Empty {}");
        match &item.kind {
            ItemKind::Struct(s) => {
                assert!(s.generics.is_none());
                assert!(s.fields.is_empty());
            }
            other => panic!("expected a struct item, got {other:?}"),
        }
    }

    #[test]
    fn parses_enum_with_unit_type_and_record_variants() {
        let item = parse_item("enum Shape { Empty, Circle: f64, Rect: { w: f64, h: f64 } }");
        match &item.kind {
            ItemKind::Enum(e) => {
                assert_eq!(text(e.name), "Shape");
                assert_eq!(e.variants.len(), 3);

                assert_eq!(text(e.variants[0].name), "Empty");
                assert!(matches!(e.variants[0].payload, VariantPayload::Unit));

                assert_eq!(text(e.variants[1].name), "Circle");
                assert!(matches!(e.variants[1].payload, VariantPayload::Type(_)));

                assert_eq!(text(e.variants[2].name), "Rect");
                match &e.variants[2].payload {
                    VariantPayload::Record(fields) => assert_eq!(fields.len(), 2),
                    other => panic!("expected a record payload, got {other:?}"),
                }
            }
            other => panic!("expected an enum item, got {other:?}"),
        }
    }

    #[test]
    fn parses_trait_with_abstract_and_provided_methods() {
        let item = parse_item("trait Greet { fun hello(&self); fun bye(&self) {} }");
        match &item.kind {
            ItemKind::Trait(t) => {
                assert_eq!(text(t.name), "Greet");
                assert_eq!(t.functions.len(), 2);
                assert!(t.functions[0].body.is_none());
                assert!(t.functions[1].body.is_some());
            }
            other => panic!("expected a trait item, got {other:?}"),
        }
    }

    #[test]
    fn parses_extend_with_trait_and_generics() {
        let item = parse_item("extend<T> Box<T> with Container<T> { fun get(&self) -> T {} }");
        match &item.kind {
            ItemKind::Extend(e) => {
                assert!(e.extend_generics.is_some());
                assert_eq!(text(e.adt_path.segments[0]), "Box");
                assert!(e.adt_generics.is_some());
                let trait_path = e.trait_path.as_ref().expect("expected a trait path");
                assert_eq!(text(trait_path.segments[0]), "Container");
                assert!(e.trait_generics.is_some());
                assert_eq!(e.methods.len(), 1);
                assert!(e.methods[0].body.is_some());
            }
            other => panic!("expected an extend item, got {other:?}"),
        }
    }

    #[test]
    fn parses_extend_without_trait() {
        let item = parse_item("extend Box<i32> { fun get(&self) {} }");
        match &item.kind {
            ItemKind::Extend(e) => {
                assert!(e.extend_generics.is_none());
                assert!(e.trait_path.is_none());
                assert!(e.trait_generics.is_none());
                assert_eq!(e.methods.len(), 1);
            }
            other => panic!("expected an extend item, got {other:?}"),
        }
    }

    #[test]
    fn parses_module_decl() {
        let item = parse_item("module math::vector;");
        match &item.kind {
            ItemKind::Module(m) => {
                assert_eq!(m.path.segments.len(), 2);
                assert_eq!(text(m.path.segments[0]), "math");
                assert_eq!(text(m.path.segments[1]), "vector");
            }
            other => panic!("expected a module item, got {other:?}"),
        }
    }

    #[test]
    fn parses_plain_import() {
        let item = parse_item("import math::vector;");
        match &item.kind {
            ItemKind::Import(i) => {
                assert_eq!(i.path.segments.len(), 2);
                assert!(!i.glob);
                assert!(i.alias.is_none());
            }
            other => panic!("expected an import item, got {other:?}"),
        }
    }

    #[test]
    fn parses_glob_import() {
        let item = parse_item("import math::*;");
        match &item.kind {
            ItemKind::Import(i) => {
                assert!(i.glob);
                assert!(i.alias.is_none());
            }
            other => panic!("expected an import item, got {other:?}"),
        }
    }

    #[test]
    fn parses_aliased_import() {
        let item = parse_item("import math::vector as mv;");
        match &item.kind {
            ItemKind::Import(i) => {
                assert!(!i.glob);
                let alias = i.alias.expect("expected an alias");
                assert_eq!(text(alias), "mv");
            }
            other => panic!("expected an import item, got {other:?}"),
        }
    }
}
