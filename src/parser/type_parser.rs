//! Parses types. A type shows up in a few places:
//!
//! - `let x: i32 = 0;`
//! - `fun add(x: i32, y: i32) -> i32 { ... }`
//! - `struct Foo { field: any Shape }`
//!
//! Array types can carry a length expression (`[i32; 5]`), so this grammar needs an expression
//! parser. [`Parser::type_parser_with_expr`] takes one as a parameter instead of building its
//! own, since `expr_parser` needs a type parser back for closures, and the two would otherwise
//! have no way to be built together.

use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::Mutability;
use crate::ast::interner::Interner;
use crate::ast::{Expr, Ident, NodeId, Path, Ty, TyKind};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl Parser {
    /// Parses a single type, using this parser's own expression parser for array lengths.
    pub fn type_parser<'a>(&'a self) -> BoxedP<'a, Ty> {
        self.type_parser_with_expr(self.expr_parser())
    }

    /// Parses a single type, using `expr` for array-length expressions (`[i32; N]`) instead of
    /// building a fresh expression parser.
    ///
    /// Callers that already have an expression parser in hand (like `expr_parser`, which needs
    /// a type parser for closure parameter types) pass it in here so the two grammars share one
    /// underlying parser instead of each building their own.
    pub(crate) fn type_parser_with_expr<'a>(&'a self, expr: BoxedP<'a, Expr>) -> BoxedP<'a, Ty> {
        recursive(
            |ty: Recursive<dyn ChumskyParser<'a, &'a [Token], Ty, Extra<'a>>>| {
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
                .map(|t: Token| Ty::primitive(t))
                .boxed();

                // A named type, with an optional generic argument list: `String`, `Option<T>`,
                // `Result<T, E>`.
                //
                // Unlike an expression, a type has no other meaning for `<`, so the argument
                // list is taken greedily wherever one follows a path. Nesting needs no special
                // handling either: the lexer only ever produces `>` as a single `CloseCaret`,
                // never a shift token, so `Array<Option<T>>` closes as two ordinary tokens.
                let path_ty = self
                    .path_parser()
                    .then(
                        self.kind(TokenKind::OpenCaret)
                            .ignore_then(
                                ty.clone()
                                    .separated_by(self.kind(TokenKind::Comma))
                                    .allow_trailing()
                                    .at_least(1)
                                    .collect::<Vec<_>>(),
                            )
                            .then(self.kind(TokenKind::CloseCaret))
                            .or_not(),
                    )
                    .map(|(p, args): (Path, Option<(Vec<Ty>, Token)>)| {
                        let (args, span) = match args {
                            Some((args, close_tok)) => (args, p.span.merge(close_tok.span)),
                            None => (Vec::new(), p.span),
                        };

                        Ty {
                            id: NodeId::next(),
                            span,
                            kind: TyKind::Path { path: p, args },
                        }
                    })
                    .boxed();

                // `Self` is an ordinary single-segment path in the AST, not its own `TyKind`:
                // the AST resolver (still to come) can then treat it like any other name instead
                // of needing a special case. HIR lowering recognizes this specific path shape and
                // maps it back to `HirTyKind::SelfType`, so everything downstream of the AST
                // keeps seeing `Self` as its own kind of type.
                let self_ty = self
                    .kind(TokenKind::UpperSelfKw)
                    .map(|self_tok| {
                        let span = self_tok.span;
                        Ty {
                            id: NodeId::next(),
                            span,
                            kind: TyKind::Path {
                                path: Path {
                                    segments: vec![Ident {
                                        text: Interner::intern("Self"),
                                        span,
                                    }],
                                    span,
                                },
                                args: Vec::new(),
                            },
                        }
                    })
                    .boxed();

                let tuple_ty = self
                    .kind(TokenKind::OpenParen)
                    .then(
                        ty.clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .at_least(1)
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((open_tok, inside_types), close_tok)| Ty {
                        id: NodeId::next(),
                        span: open_tok.span.merge(close_tok.span),
                        kind: TyKind::Tuple(inside_types.into_iter().collect::<Vec<_>>()),
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
                    .map(|(((open_tok, elem_ty), len), close_tok)| Ty {
                        id: NodeId::next(),
                        span: open_tok.span.merge(close_tok.span),
                        kind: TyKind::Array {
                            elem: Box::new(elem_ty),
                            len: len.map(Box::new),
                        },
                    })
                    .boxed();

                // `any` may only wrap a base (primitive or path), tuple, array, or `Self` type.
                // It can never wrap a reference, `dyn`, or another `any`.
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
                    .map(|(any_tok, inner_ty)| Ty {
                        id: NodeId::next(),
                        span: any_tok.span.merge(inner_ty.span),
                        kind: TyKind::Any(Box::new(inner_ty)),
                    })
                    .boxed();

                let ref_ty = self
                    .kind(TokenKind::Amp)
                    .then(self.kind(TokenKind::MutKw).or_not())
                    .then(ty.clone())
                    .map(|((amp_tok, mut_tok), ty)| {
                        let mutability = if mut_tok.is_some() {
                            Mutability::Mutable
                        } else {
                            Mutability::Immutable
                        };

                        Ty {
                            id: NodeId::next(),
                            span: amp_tok.span.merge(ty.span),
                            kind: TyKind::Ref {
                                base: Box::new(ty),
                                mutability,
                            },
                        }
                    })
                    .boxed();

                // `dyn Trait`, with the same optional argument list a named type takes: a trait
                // that declares parameters has to be applied to them before it names a type, so
                // `dyn Index<K, V>` is as ordinary as `Map<K, V>`.
                let dyn_ty = self
                    .kind(TokenKind::DynKw)
                    .then(self.path_parser())
                    .then(
                        self.kind(TokenKind::OpenCaret)
                            .ignore_then(
                                ty.clone()
                                    .separated_by(self.kind(TokenKind::Comma))
                                    .allow_trailing()
                                    .at_least(1)
                                    .collect::<Vec<_>>(),
                            )
                            .then(self.kind(TokenKind::CloseCaret))
                            .or_not(),
                    )
                    .map(
                        |((dyn_tok, path), args): ((Token, Path), Option<(Vec<Ty>, Token)>)| {
                            let (args, end) = match args {
                                Some((args, close_tok)) => (args, close_tok.span),
                                None => (Vec::new(), path.span),
                            };

                            Ty {
                                id: NodeId::next(),
                                span: dyn_tok.span.merge(end),
                                kind: TyKind::Dyn { path, args },
                            }
                        },
                    )
                    .boxed();

                // A function type looks like `fun(i32, i32) -> i32` or `fun(&str)`. Omitting
                // `->` means the function returns no value.
                let fn_ty = self
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
                        Ty {
                            id: NodeId::next(),
                            span: fun_tok.span.merge(end_span),
                            kind: TyKind::Function {
                                params: params.into_iter().collect(),
                                ret: ret.map(|t| Box::new(t)),
                            },
                        }
                    })
                    .boxed();

                choice((
                    self_ty,
                    dyn_ty,
                    any_ty,
                    ref_ty,
                    fn_ty,
                    primitive_ty,
                    tuple_ty,
                    array_ty,
                    path_ty,
                ))
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
        for src in ["i32", "u64", "f64", "bool", "char", "str"] {
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

    /// `>>` is two `CloseCaret` tokens rather than a shift, so a nested argument list needs no
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
    fn self_type_parses_as_a_single_segment_path() {
        let ty = parse_ty("Self");
        let TyKind::Path { path, args } = &ty.kind else {
            panic!("expected `Self` to parse as a path, got {:?}", ty.kind);
        };
        assert_eq!(path.segments.len(), 1);
        assert_eq!(Interner::resolve(path.segments[0].text), "Self");
        assert!(args.is_empty());
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

    #[test]
    fn parses_ref_to_ref_type() {
        // `&mut &i32` is a mutable reference to an immutable reference. `&&i32` can't be
        // spelled this way, since the lexer tokenizes `&&` as a single `DoubleAmp` token.
        let ty = parse_ty("&mut &i32");
        match &ty.kind {
            TyKind::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                match &base.kind {
                    TyKind::Ref { mutability, base } => {
                        assert!(matches!(mutability, Mutability::Immutable));
                        assert!(matches!(base.kind, TyKind::Path { .. }));
                    }
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
            TyKind::Any(inner) => match &inner.kind {
                TyKind::Path { path, args } => {
                    assert_eq!(path.segments.len(), 1);
                    assert_eq!(Interner::resolve(path.segments[0].text), "Self");
                    assert!(args.is_empty());
                }
                other => panic!("expected `Self` to parse as a path, got {other:?}"),
            },
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn rejects_any_wrapping_a_ref_type() {
        assert_eq!(diagnostic_count("any &i32"), 1);
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
