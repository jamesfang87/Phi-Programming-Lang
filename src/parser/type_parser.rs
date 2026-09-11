use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::Mutability;
use crate::ast::{Expr, Path, Ty, TyKind};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

type TypeArgList = Option<(Vec<Ty>, Token)>;

impl Parser {
    pub fn type_parser<'a>(&'a self) -> BoxedP<'a, Ty> {
        self.type_parser_with_expr(self.expr_parser())
    }

    pub(crate) fn primitive_token_parser<'a>(&'a self) -> BoxedP<'a, Token> {
        choice((
            self.kind(TokenKind::I8),
            self.kind(TokenKind::I16),
            self.kind(TokenKind::I32),
            self.kind(TokenKind::I64),
            self.kind(TokenKind::U8),
            self.kind(TokenKind::U16),
            self.kind(TokenKind::U32),
            self.kind(TokenKind::U64),
            self.kind(TokenKind::Usize),
            self.kind(TokenKind::F32),
            self.kind(TokenKind::F64),
            self.kind(TokenKind::BoolKw),
            self.kind(TokenKind::Char),
            self.kind(TokenKind::Str),
        ))
        .boxed()
    }

    pub(crate) fn type_parser_with_expr<'a>(&'a self, expr: BoxedP<'a, Expr>) -> BoxedP<'a, Ty> {
        recursive(
            |ty: Recursive<dyn ChumskyParser<'a, &'a [Token], Ty, Extra<'a>>>| {
                let primitive_ty = self
                    .primitive_token_parser()
                    .map(|t: Token| Ty::primitive(t))
                    .boxed();

                let path_ty = self
                    .path_parser()
                    .then(
                        self.kind(TokenKind::OpenAngle)
                            .ignore_then(
                                ty.clone()
                                    .separated_by(self.kind(TokenKind::Comma))
                                    .allow_trailing()
                                    .at_least(1)
                                    .collect::<Vec<_>>(),
                            )
                            .then(self.kind(TokenKind::CloseAngle))
                            .or_not(),
                    )
                    .map(|(p, args): (Path, TypeArgList)| {
                        let (args, span) = match args {
                            Some((args, close_tok)) => (args, p.span.merge(close_tok.span)),
                            None => (Vec::new(), p.span),
                        };

                        Ty::new(TyKind::Path { path: p, args }, span)
                    })
                    .boxed();

                let self_ty = self
                    .kind(TokenKind::UpperSelfKw)
                    .map(|self_tok| {
                        let span = self_tok.span;
                        Ty::new(TyKind::SelfTy, span)
                    })
                    .boxed();

                let tuple_ty = self
                    .kind(TokenKind::OpenParen)
                    .then(
                        ty.clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .at_least(1)
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((open_tok, inside_types), close_tok)| {
                        Ty::new(
                            TyKind::Tuple(inside_types.into_iter().collect::<Vec<_>>()),
                            open_tok.span.merge(close_tok.span),
                        )
                    })
                    .boxed();

                let array_ty = self
                    .kind(TokenKind::OpenBracket)
                    .then(ty.clone())
                    .then(
                        self.kind(TokenKind::Semicolon)
                            .ignore_then(expr.clone())
                            .or_not(),
                    )
                    .then(self.kind(TokenKind::CloseBracket))
                    .map(|(((open_tok, elem_ty), len), close_tok)| {
                        Ty::new(
                            TyKind::Array {
                                elem: Box::new(elem_ty),
                                len: len.map(Box::new),
                            },
                            open_tok.span.merge(close_tok.span),
                        )
                    })
                    .boxed();

                let any_target = choice((
                    self_ty.clone(),
                    primitive_ty.clone(),
                    path_ty.clone(),
                    tuple_ty.clone(),
                    array_ty.clone(),
                ))
                .boxed();

                let any_ty = self
                    .kind(TokenKind::AnyKw)
                    .then(any_target)
                    .map(|(any_tok, inner_ty)| {
                        let span = any_tok.span.merge(inner_ty.span);
                        Ty::new(TyKind::Any(Box::new(inner_ty)), span)
                    })
                    .boxed();

                let dyn_ty = self
                    .kind(TokenKind::DynKw)
                    .then(self.path_parser())
                    .then(
                        self.kind(TokenKind::OpenAngle)
                            .ignore_then(
                                ty.clone()
                                    .separated_by(self.kind(TokenKind::Comma))
                                    .allow_trailing()
                                    .at_least(1)
                                    .collect::<Vec<_>>(),
                            )
                            .then(self.kind(TokenKind::CloseAngle))
                            .or_not(),
                    )
                    .map(|((dyn_tok, path), args): ((Token, Path), TypeArgList)| {
                        let (args, end) = match args {
                            Some((args, close_tok)) => (args, close_tok.span),
                            None => (Vec::new(), path.span),
                        };

                        Ty::new(TyKind::Dyn { path, args }, dyn_tok.span.merge(end))
                    })
                    .boxed();

                let fun_ty = self
                    .kind(TokenKind::FunKw)
                    .then_ignore(self.kind(TokenKind::OpenParen))
                    .then(
                        ty.clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .then(self.kind(TokenKind::Arrow).ignore_then(ty.clone()).or_not())
                    .map(|(((fun_tok, params), close_tok), ret)| {
                        let end_span = match &ret {
                            Some(ret) => ret.span,
                            None => close_tok.span,
                        };
                        Ty::new(
                            TyKind::Function {
                                params: params.into_iter().collect(),
                                ret: ret.map(Box::new),
                            },
                            fun_tok.span.merge(end_span),
                        )
                    })
                    .boxed();

                let iso_target = choice((
                    self_ty.clone(),
                    primitive_ty.clone(),
                    path_ty.clone(),
                    tuple_ty.clone(),
                    array_ty.clone(),
                    dyn_ty.clone(),
                ))
                .boxed();

                let iso_ty = self
                    .kind(TokenKind::IsoKw)
                    .then(iso_target)
                    .map(|(iso_tok, inner_ty)| {
                        let span = iso_tok.span.merge(inner_ty.span);
                        Ty::new(TyKind::Iso(Box::new(inner_ty)), span)
                    })
                    .boxed();

                let ref_ty = recursive(
                    |ref_ty: Recursive<dyn ChumskyParser<'a, &'a [Token], Ty, Extra<'a>>>| {
                        let ref_target = choice((
                            self_ty.clone(),
                            dyn_ty.clone(),
                            iso_ty.clone(),
                            fun_ty.clone(),
                            primitive_ty.clone(),
                            tuple_ty.clone(),
                            array_ty.clone(),
                            path_ty.clone(),
                            ref_ty.clone(),
                        ))
                        .boxed();

                        let single_amp = self
                            .kind(TokenKind::Amp)
                            .then(self.kind(TokenKind::MutKw).or_not())
                            .then(ref_target.clone())
                            .map(|((amp_tok, mut_tok), ty)| {
                                let mutability = if mut_tok.is_some() {
                                    Mutability::Mutable
                                } else {
                                    Mutability::Immutable
                                };

                                let span = amp_tok.span.merge(ty.span);
                                Ty::new(
                                    TyKind::Ref {
                                        base: Box::new(ty),
                                        mutability,
                                    },
                                    span,
                                )
                            })
                            .boxed();

                        let double_amp = self
                            .kind(TokenKind::DoubleAmp)
                            .then(self.kind(TokenKind::MutKw).or_not())
                            .then(ref_target)
                            .map(|((amp_tok, mut_tok), ty)| {
                                let inner_mutability = if mut_tok.is_some() {
                                    Mutability::Mutable
                                } else {
                                    Mutability::Immutable
                                };

                                let inner_span = amp_tok.span.merge(ty.span);
                                let inner = Ty::new(
                                    TyKind::Ref {
                                        base: Box::new(ty),
                                        mutability: inner_mutability,
                                    },
                                    inner_span,
                                );

                                let outer_span = amp_tok.span.merge(inner.span);
                                Ty::new(
                                    TyKind::Ref {
                                        base: Box::new(inner),
                                        mutability: Mutability::Immutable,
                                    },
                                    outer_span,
                                )
                            })
                            .boxed();

                        choice((double_amp, single_amp)).boxed()
                    },
                )
                .boxed();

                choice((
                    self_ty,
                    dyn_ty,
                    any_ty,
                    iso_ty,
                    ref_ty,
                    fun_ty,
                    primitive_ty,
                    tuple_ty,
                    array_ty,
                    path_ty,
                ))
                .labelled("a type")
                .boxed()
            },
        )
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::interner::Interner;
    use crate::ast::{ExprKind, Literal};
    use crate::driver::source::SrcMap;
    use crate::lexer::Lexer;
    use crate::testing::lex_src;

    fn parse_ty(src: &str) -> Ty {
        let (tokens, _) = lex_src(src);
        let parser = Parser::new();
        let (output, errors) = parser.type_parser().parse(&tokens[..]).into_output_errors();
        assert!(
            errors.is_empty(),
            "unexpected parse errors for {src:?}: {errors:?}"
        );
        output.expect("expected a successfully parsed type")
    }

    /// Parses `src` as a type, returning how many parse errors were raised (without asserting
    /// they're empty, unlike [`parse_ty`]).
    fn diagnostic_count(src: &str) -> usize {
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file(
            "<test>".to_string(),
            chars.clone(),
            crate::driver::source::FileOrigin::User,
        );
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new();
        let (_, errors) = parser
            .type_parser()
            .then(end())
            .parse(&tokens[..])
            .into_output_errors();
        errors.len()
    }

    fn base_name(ty: &Ty) -> &'static str {
        match &ty.kind {
            TyKind::Path { path, .. } => Interner::resolve(path.segments[0].text),
            other => panic!("expected a base type, got {other:?}"),
        }
    }

    #[test]
    fn parses_primitive_types() {
        for src in ["i32", "u64", "usize", "f64", "bool", "char", "str"] {
            let ty = parse_ty(src);
            assert_eq!(base_name(&ty), src);
        }
    }

    #[test]
    fn parses_qualified_path_type() {
        let ty = parse_ty("math::Vector2D");
        match &ty.kind {
            TyKind::Path { path, args } => {
                assert_eq!(path.segments.len(), 2);
                assert_eq!(Interner::resolve(path.segments[0].text), "math");
                assert_eq!(Interner::resolve(path.segments[1].text), "Vector2D");
                assert!(args.is_empty());
            }
            other => panic!("expected a base type, got {other:?}"),
        }
    }

    #[test]
    fn parses_generic_args_on_a_named_type() {
        let ty = parse_ty("Result<T, E>");
        match &ty.kind {
            TyKind::Path { path, args } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Result");
                assert_eq!(args.len(), 2);
                assert_eq!(base_name(&args[0]), "T");
                assert_eq!(base_name(&args[1]), "E");
            }
            other => panic!("expected a base type, got {other:?}"),
        }
    }

    /// `>>` is two `CloseAngle` tokens rather than a shift, so a nested argument list needs no
    /// special handling to close.
    #[test]
    fn parses_nested_generic_args() {
        let ty = parse_ty("Array<Option<i32>>");
        match &ty.kind {
            TyKind::Path { path, args } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Array");
                assert_eq!(args.len(), 1);
                match &args[0].kind {
                    TyKind::Path { path, args } => {
                        assert_eq!(Interner::resolve(path.segments[0].text), "Option");
                        assert_eq!(args.len(), 1);
                    }
                    other => panic!("expected a nested base type, got {other:?}"),
                }
            }
            other => panic!("expected a base type, got {other:?}"),
        }
    }

    #[test]
    fn parses_immutable_ref_type() {
        let ty = parse_ty("&i32");
        match &ty.kind {
            TyKind::Ref { mutability, .. } => assert!(matches!(mutability, Mutability::Immutable)),
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_ref_type() {
        let ty = parse_ty("&mut i32");
        match &ty.kind {
            TyKind::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                assert!(matches!(base.kind, TyKind::Path { .. }));
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_type() {
        let ty = parse_ty("any i32");
        match &ty.kind {
            TyKind::Any(inner) => assert!(matches!(inner.kind, TyKind::Path { .. })),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_dyn_type() {
        let ty = parse_ty("dyn Shape");
        match &ty.kind {
            TyKind::Dyn { path, args } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Shape");
                assert!(args.is_empty());
            }
            other => panic!("expected a dyn type, got {other:?}"),
        }
    }

    /// A trait that declares parameters has to be applied to them before `dyn` names a type.
    #[test]
    fn parses_dyn_type_with_generic_arguments() {
        let ty = parse_ty("dyn Index<K, V>");
        match &ty.kind {
            TyKind::Dyn { path, args } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Index");
                assert_eq!(args.len(), 2);
            }
            other => panic!("expected a dyn type, got {other:?}"),
        }
    }

    /// The argument list binds to the `dyn`, not to something after it: a `dyn` inside a
    /// reference still ends where its own `>` does.
    #[test]
    fn parses_a_reference_to_a_generic_dyn() {
        let ty = parse_ty("&dyn Index<K, V>");
        match &ty.kind {
            TyKind::Ref { base, .. } => match &base.kind {
                TyKind::Dyn { args, .. } => assert_eq!(args.len(), 2),
                other => panic!("expected a dyn type, got {other:?}"),
            },
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn self_type_parses_as_its_own_kind() {
        let ty = parse_ty("Self");
        assert!(matches!(ty.kind, TyKind::SelfTy));
    }

    #[test]
    fn parses_tuple_type() {
        let ty = parse_ty("(i32, bool)");
        match &ty.kind {
            TyKind::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert!(matches!(types[0].kind, TyKind::Path { .. }));
                assert!(matches!(types[1].kind, TyKind::Path { .. }));
            }
            other => panic!("expected a tuple type, got {other:?}"),
        }
    }

    /// A one-element tuple type needs the trailing comma, mirroring the expression and
    /// pattern grammars: `(T,)` is a tuple, `(T)` is just `T`.
    #[test]
    fn parses_one_element_tuple_type_with_trailing_comma() {
        let ty = parse_ty("(i32,)");
        match &ty.kind {
            TyKind::Tuple(types) => assert_eq!(types.len(), 1),
            other => panic!("expected a tuple type, got {other:?}"),
        }
    }

    /// The trailing comma alone is not a type: parens around nothing are not a tuple here.
    #[test]
    fn rejects_empty_tuple_type() {
        assert_eq!(diagnostic_count("()"), 1);
    }

    #[test]
    fn parses_array_type_without_length() {
        let ty = parse_ty("[i32]");
        match &ty.kind {
            TyKind::Array { elem, len } => {
                assert!(matches!(elem.kind, TyKind::Path { .. }));
                assert!(len.is_none());
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_type_with_length() {
        let ty = parse_ty("[i32; 5]");
        match &ty.kind {
            TyKind::Array { elem, len } => {
                assert!(matches!(elem.kind, TyKind::Path { .. }));
                let len = len.as_ref().expect("expected an array length");
                assert!(matches!(len.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    /// A reference may wrap another reference (`&&T`): the outer reference's target is itself a
    /// reference type. `&mut &i32` reaches that shape through two separate `&` tokens.
    #[test]
    fn parses_ref_wrapping_a_ref_type_via_two_amp_tokens() {
        let ty = parse_ty("&mut &i32");
        match &ty.kind {
            TyKind::Ref {
                mutability: outer_mutability,
                base,
            } => {
                assert!(matches!(outer_mutability, Mutability::Mutable));
                match &base.kind {
                    TyKind::Ref {
                        mutability: inner_mutability,
                        base: inner_base,
                    } => {
                        assert!(matches!(inner_mutability, Mutability::Immutable));
                        assert!(matches!(inner_base.kind, TyKind::Path { .. }));
                    }
                    other => panic!("expected a nested ref type, got {other:?}"),
                }
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    /// The same shape, but spelled the natural way: the lexer tokenizes `&&` as a single
    /// `DoubleAmp` token (it's also the logical-and operator in expression position), so the
    /// type parser must split it into two reference layers itself rather than requiring a space.
    #[test]
    fn parses_ref_wrapping_a_ref_type_via_double_amp_token() {
        let ty = parse_ty("&&i32");
        match &ty.kind {
            TyKind::Ref {
                mutability: outer_mutability,
                base,
            } => {
                assert!(matches!(outer_mutability, Mutability::Immutable));
                match &base.kind {
                    TyKind::Ref {
                        mutability: inner_mutability,
                        base: inner_base,
                    } => {
                        assert!(matches!(inner_mutability, Mutability::Immutable));
                        assert!(matches!(inner_base.kind, TyKind::Path { .. }));
                    }
                    other => panic!("expected a nested ref type, got {other:?}"),
                }
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    /// `&&mut i32` is a valid shape too: an immutable outer reference (from the `DoubleAmp`
    /// token) to a mutable inner reference.
    #[test]
    fn parses_ref_wrapping_a_mutable_ref_type_via_double_amp_token() {
        let ty = parse_ty("&&mut i32");
        match &ty.kind {
            TyKind::Ref {
                mutability: outer_mutability,
                base,
            } => {
                assert!(matches!(outer_mutability, Mutability::Immutable));
                match &base.kind {
                    TyKind::Ref {
                        mutability: inner_mutability,
                        ..
                    } => assert!(matches!(inner_mutability, Mutability::Mutable)),
                    other => panic!("expected a nested ref type, got {other:?}"),
                }
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_ref_to_array_type() {
        let ty = parse_ty("&mut [i32]");
        match &ty.kind {
            TyKind::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                assert!(matches!(base.kind, TyKind::Array { .. }));
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tuple_type() {
        let ty = parse_ty("(i32, (bool, char))");
        match &ty.kind {
            TyKind::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert!(matches!(types[0].kind, TyKind::Path { .. }));
                match &types[1].kind {
                    TyKind::Tuple(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(inner[0].kind, TyKind::Path { .. }));
                        assert!(matches!(inner[1].kind, TyKind::Path { .. }));
                    }
                    other => panic!("expected a nested tuple type, got {other:?}"),
                }
            }
            other => panic!("expected a tuple type, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_of_arrays_type() {
        let ty = parse_ty("[[i32]]");
        match &ty.kind {
            TyKind::Array { elem, len } => {
                assert!(matches!(elem.kind, TyKind::Array { .. }));
                assert!(len.is_none());
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_of_tuples_with_ref_element_type() {
        // `[(&i32, bool); 3]` exercises array + tuple + ref nesting together.
        let ty = parse_ty("[(&i32, bool); 3]");
        match &ty.kind {
            TyKind::Array { elem, len } => {
                match &elem.kind {
                    TyKind::Tuple(types) => {
                        assert_eq!(types.len(), 2);
                        assert!(matches!(types[0].kind, TyKind::Ref { .. }));
                        assert!(matches!(types[1].kind, TyKind::Path { .. }));
                    }
                    other => panic!("expected a tuple element type, got {other:?}"),
                }
                let len = len.as_ref().expect("expected an array length");
                assert!(matches!(len.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_tuple_type() {
        let ty = parse_ty("any (i32, bool)");
        match &ty.kind {
            TyKind::Any(inner) => assert!(matches!(inner.kind, TyKind::Tuple(_))),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_array_type() {
        let ty = parse_ty("any [i32; 4]");
        match &ty.kind {
            TyKind::Any(inner) => assert!(matches!(inner.kind, TyKind::Array { .. })),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_self_type() {
        let ty = parse_ty("any Self");
        match &ty.kind {
            TyKind::Any(inner) => assert!(matches!(inner.kind, TyKind::SelfTy)),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn rejects_any_wrapping_a_ref_type() {
        assert_eq!(diagnostic_count("any &i32"), 1);
    }

    /// The other direction is rejected too: a reference may not wrap `any`. `any` describes how
    /// a value crosses a function boundary; layering a reference on top of that would just be a
    /// second, redundant indirection.
    #[test]
    fn rejects_ref_wrapping_an_any_type() {
        assert_eq!(diagnostic_count("&any i32"), 1);
    }

    #[test]
    fn rejects_any_wrapping_a_dyn_type() {
        assert_eq!(diagnostic_count("any dyn Shape"), 1);
    }

    #[test]
    fn rejects_any_wrapping_another_any_type() {
        assert_eq!(diagnostic_count("any any i32"), 1);
    }

    #[test]
    fn parses_iso_type() {
        let ty = parse_ty("iso i32");
        match &ty.kind {
            TyKind::Iso(inner) => assert!(matches!(inner.kind, TyKind::Path { .. })),
            other => panic!("expected an iso type, got {other:?}"),
        }
    }

    #[test]
    fn rejects_iso_wrapping_a_ref_type() {
        assert_eq!(diagnostic_count("iso &i32"), 1);
    }

    /// `iso` is this language's owning pointer -- Rust's `Box` -- so `iso dyn Trait` is the
    /// usual way to own an unsized trait object, the same shape as `Box<dyn Trait>`.
    #[test]
    fn parses_iso_dyn_type() {
        let ty = parse_ty("iso dyn Shape");
        match &ty.kind {
            TyKind::Iso(inner) => assert!(matches!(inner.kind, TyKind::Dyn { .. })),
            other => panic!("expected an iso type, got {other:?}"),
        }
    }

    #[test]
    fn rejects_iso_wrapping_an_any_type() {
        assert_eq!(diagnostic_count("iso any i32"), 1);
    }

    #[test]
    fn rejects_iso_wrapping_a_fn_type() {
        assert_eq!(diagnostic_count("iso fun(i32) -> i32"), 1);
    }

    #[test]
    fn parses_fn_type_with_params_and_return_type() {
        let ty = parse_ty("fun(i32, i32) -> i32");
        match &ty.kind {
            TyKind::Function { params, ret } => {
                assert_eq!(params.len(), 2);
                assert!(matches!(params[0].kind, TyKind::Path { .. }));
                assert!(ret.is_some());
                assert!(matches!(ret.clone().unwrap().kind, TyKind::Path { .. }));
            }
            other => panic!("expected a fn type, got {other:?}"),
        }
    }

    #[test]
    fn parses_fn_type_with_no_params_and_no_return_type() {
        let ty = parse_ty("fun()");
        match &ty.kind {
            TyKind::Function { params, ret } => {
                assert!(params.is_empty());
                assert!(ret.is_none());
            }
            other => panic!("expected a fn type, got {other:?}"),
        }
    }

    #[test]
    fn parses_fn_type_with_ref_param() {
        let ty = parse_ty("fun(&str)");
        match &ty.kind {
            TyKind::Function { params, .. } => {
                assert_eq!(params.len(), 1);
                assert!(matches!(params[0].kind, TyKind::Ref { .. }));
            }
            other => panic!("expected a fn type, got {other:?}"),
        }
    }

    #[test]
    fn parses_higher_order_fn_type() {
        // A fn type whose parameter and return type are themselves fn types.
        let ty = parse_ty("fun(fun(i32) -> i32) -> fun() -> bool");
        match &ty.kind {
            TyKind::Function { params, ret } => {
                assert_eq!(params.len(), 1);
                assert!(matches!(params[0].kind, TyKind::Function { .. }));
                match &ret.clone().map(|t| t.kind) {
                    Some(TyKind::Function { .. }) => {}
                    other => panic!("expected a fn return type, got {other:?}"),
                }
            }
            other => panic!("expected a fn type, got {other:?}"),
        }
    }

    #[test]
    fn rejects_any_wrapping_a_fn_type() {
        assert_eq!(diagnostic_count("any fun(i32) -> i32"), 1);
    }
}
