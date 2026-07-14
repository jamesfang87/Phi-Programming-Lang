use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::Mutability;
use crate::ast::{Expr, Path, Ty, Type};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl Parser {
    pub fn type_parser<'a>(&'a self) -> BoxedP<'a, Type> {
        self.type_parser_with_expr(self.expr_parser())
    }

    /// The real implementation, parameterized over the expr parser used for array lengths.
    ///
    /// `type_parser()` builds its own (via `self.expr_parser()`); `expr_and_block_parsers()` uses
    /// this directly instead, passing in the `expr` handle it already tied together with `block`
    /// — calling `self.expr_parser()` from there would recurse infinitely at grammar-construction
    /// time, since that in turn builds a block parser whose `let` statements need a type parser.
    pub(crate) fn type_parser_with_expr<'a>(&'a self, expr: BoxedP<'a, Expr>) -> BoxedP<'a, Type> {
        recursive(
            |ty: Recursive<dyn ChumskyParser<'a, &'a [Token], Type, Extra<'a>>>| {
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

                let path_ty = self
                    .path_parser()
                    .map(|p: Path| Type {
                        span: p.span,
                        kind: Ty::Base {
                            base: p,
                            args: Vec::new(),
                        },
                    })
                    .boxed();

                let self_ty = self
                    .kind(TokenKind::UpperSelfKw)
                    .map(|self_tok| Type {
                        span: self_tok.span,
                        kind: Ty::SelfType,
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
                    .map(|((open_tok, inside_types), close_tok)| Type {
                        span: open_tok.span.merge(close_tok.span),
                        kind: Ty::Tuple(
                            (inside_types.into_iter().map(|t| t.kind)).collect::<Vec<_>>(),
                        ),
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
                    .map(|(((open_tok, elem_ty), len), close_tok)| Type {
                        span: open_tok.span.merge(close_tok.span),
                        kind: Ty::Array {
                            elem: Box::new(elem_ty.kind),
                            len: len.map(Box::new),
                        },
                    })
                    .boxed();

                // `any` may only wrap a base (primitive or path), tuple, array, or `Self` type —
                // never a reference, `dyn`, or another `any`.
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
                    .map(|(any_tok, inner_ty)| Type {
                        span: any_tok.span.merge(inner_ty.span),
                        kind: Ty::Any(Box::new(inner_ty.kind)),
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

                        Type {
                            span: amp_tok.span.merge(ty.span),
                            kind: Ty::Ref {
                                base: Box::new(ty.kind),
                                mutability,
                            },
                        }
                    })
                    .boxed();

                let dyn_ty = self
                    .kind(TokenKind::DynKw)
                    .then(self.path_parser())
                    .map(|(dyn_tok, path)| Type {
                        span: dyn_tok.span.merge(path.span),
                        kind: Ty::Dyn(path),
                    })
                    .boxed();

                choice((
                    self_ty,
                    dyn_ty,
                    any_ty,
                    ref_ty,
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
    use crate::ast::{ExprKind, Literal};
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::interner::Interner;
    use crate::lexer::Lexer;

    fn parse_ty(src: &str) -> Type {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
        let (output, errors) = parser
            .type_parser()
            .parse(&tokens[..])
            .into_output_errors();
        assert!(errors.is_empty(), "unexpected parse errors for {src:?}: {errors:?}");
        output.expect("expected a successfully parsed type")
    }

    /// Parses `src` as a type, returning how many parse errors were raised (without asserting
    /// they're empty, unlike [`parse_ty`]).
    fn diagnostic_count(src: &str) -> usize {
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
        let (_, errors) = parser
            .type_parser()
            .then(end())
            .parse(&tokens[..])
            .into_output_errors();
        errors.len()
    }

    fn base_name(ty: &Type) -> String {
        match &ty.kind {
            Ty::Base { base, .. } => Interner::resolve(base.segments[0].text),
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
            Ty::Base { base, args } => {
                assert_eq!(base.segments.len(), 2);
                assert_eq!(Interner::resolve(base.segments[0].text), "math");
                assert_eq!(Interner::resolve(base.segments[1].text), "Vector2D");
                assert!(args.is_empty());
            }
            other => panic!("expected a base type, got {other:?}"),
        }
    }

    #[test]
    fn parses_immutable_ref_type() {
        let ty = parse_ty("&i32");
        match &ty.kind {
            Ty::Ref { mutability, .. } => assert!(matches!(mutability, Mutability::Immutable)),
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_ref_type() {
        let ty = parse_ty("&mut i32");
        match &ty.kind {
            Ty::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                assert!(matches!(**base, Ty::Base { .. }));
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_type() {
        let ty = parse_ty("any i32");
        match &ty.kind {
            Ty::Any(inner) => assert!(matches!(**inner, Ty::Base { .. })),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_dyn_type() {
        let ty = parse_ty("dyn Shape");
        match &ty.kind {
            Ty::Dyn(path) => assert_eq!(Interner::resolve(path.segments[0].text), "Shape"),
            other => panic!("expected a dyn type, got {other:?}"),
        }
    }

    #[test]
    fn parses_self_type() {
        let ty = parse_ty("Self");
        assert!(matches!(ty.kind, Ty::SelfType));
    }

    #[test]
    fn parses_tuple_type() {
        let ty = parse_ty("(i32, bool)");
        match &ty.kind {
            Ty::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert!(matches!(types[0], Ty::Base { .. }));
                assert!(matches!(types[1], Ty::Base { .. }));
            }
            other => panic!("expected a tuple type, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_type_without_length() {
        let ty = parse_ty("[i32]");
        match &ty.kind {
            Ty::Array { elem, len } => {
                assert!(matches!(**elem, Ty::Base { .. }));
                assert!(len.is_none());
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    #[test]
    fn parses_array_type_with_length() {
        let ty = parse_ty("[i32; 5]");
        match &ty.kind {
            Ty::Array { elem, len } => {
                assert!(matches!(**elem, Ty::Base { .. }));
                let len = len.as_ref().expect("expected an array length");
                assert!(matches!(len.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected an array type, got {other:?}"),
        }
    }

    #[test]
    fn parses_ref_to_ref_type() {
        // `&mut &i32` — a mutable reference to an immutable reference. (`&&i32` can't be spelled
        // this way since the lexer tokenizes `&&` as a single `DoubleAmp` token.)
        let ty = parse_ty("&mut &i32");
        match &ty.kind {
            Ty::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                match base.as_ref() {
                    Ty::Ref { mutability, base } => {
                        assert!(matches!(mutability, Mutability::Immutable));
                        assert!(matches!(**base, Ty::Base { .. }));
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
            Ty::Ref { mutability, base } => {
                assert!(matches!(mutability, Mutability::Mutable));
                assert!(matches!(**base, Ty::Array { .. }));
            }
            other => panic!("expected a ref type, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tuple_type() {
        let ty = parse_ty("(i32, (bool, char))");
        match &ty.kind {
            Ty::Tuple(types) => {
                assert_eq!(types.len(), 2);
                assert!(matches!(types[0], Ty::Base { .. }));
                match &types[1] {
                    Ty::Tuple(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(inner[0], Ty::Base { .. }));
                        assert!(matches!(inner[1], Ty::Base { .. }));
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
            Ty::Array { elem, len } => {
                assert!(matches!(**elem, Ty::Array { .. }));
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
            Ty::Array { elem, len } => {
                match elem.as_ref() {
                    Ty::Tuple(types) => {
                        assert_eq!(types.len(), 2);
                        assert!(matches!(types[0], Ty::Ref { .. }));
                        assert!(matches!(types[1], Ty::Base { .. }));
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
            Ty::Any(inner) => assert!(matches!(**inner, Ty::Tuple(_))),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_array_type() {
        let ty = parse_ty("any [i32; 4]");
        match &ty.kind {
            Ty::Any(inner) => assert!(matches!(**inner, Ty::Array { .. })),
            other => panic!("expected an any type, got {other:?}"),
        }
    }

    #[test]
    fn parses_any_self_type() {
        let ty = parse_ty("any Self");
        match &ty.kind {
            Ty::Any(inner) => assert!(matches!(**inner, Ty::SelfType)),
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
}
