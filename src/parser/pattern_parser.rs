//! Parses patterns. A pattern shows up in a few places:
//!
//! - `let (x, y) = point;`
//! - `for item in items { ... }`
//! - `match shape { .circle(r) => ..., _ => ... }`
//!
//! A pattern is its own small recursive grammar, separate from expressions, but it reuses the
//! expression literal parsers so `match x { 1 => ... }` accepts the same literal forms as an
//! expression would.

use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{Expr, ExprKind, Ident, Literal, Pattern, PatternKind, Payload, PayloadField};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl Parser {
    /// Parses a single pattern: a wildcard, a literal, a tuple, a variant, or a binding.
    pub fn pattern_parser<'a>(&'a self) -> BoxedP<'a, Pattern> {
        let ident = self.ident_parser();

        recursive(
            |pattern: Recursive<dyn ChumskyParser<'a, &'a [Token], Pattern, Extra<'a>>>| {
                let wildcard = self
                    .kind(TokenKind::Wildcard)
                    .map(|t: Token| Pattern {
                        kind: PatternKind::Wildcard,
                        span: t.span,
                    })
                    .boxed();

                let literal = choice((
                    self.kind(TokenKind::IntLiteral).map(|t| Expr::int(t, t)),
                    self.kind(TokenKind::FloatLiteral)
                        .map(|t| Expr::float(t, t)),
                    self.kind(TokenKind::StrLiteral).map(|t| Expr::string(t)),
                    self.kind(TokenKind::CharLiteral).map(|t| Expr::char(t)),
                    self.kind(TokenKind::TrueKw).map(|t: Token| Expr {
                        kind: ExprKind::Literal(Literal::Bool(true)),
                        span: t.span,
                    }),
                    self.kind(TokenKind::FalseKw).map(|t: Token| Expr {
                        kind: ExprKind::Literal(Literal::Bool(false)),
                        span: t.span,
                    }),
                ))
                .map(|e: Expr| {
                    let lit = match e.kind {
                        ExprKind::Literal(lit) => lit,
                        _ => unreachable!("literal parsers only ever produce `ExprKind::Literal`"),
                    };
                    Pattern {
                        kind: PatternKind::Literal(lit),
                        span: e.span,
                    }
                })
                .boxed();

                let tuple = self
                    .kind(TokenKind::OpenParen)
                    .then(
                        pattern
                            .clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((open_tok, pats), close_tok)| Pattern {
                        kind: PatternKind::Tuple(pats),
                        span: open_tok.span.merge(close_tok.span),
                    })
                    .boxed();

                // `{ l }` binds the field to its own name. `{ l: <pat> }` destructures it
                // further with a nested pattern.
                let payload_field = ident
                    .clone()
                    .then(
                        self.kind(TokenKind::Colon)
                            .ignore_then(pattern.clone())
                            .or_not(),
                    )
                    .map(|(name, value)| {
                        let span = match &value {
                            Some(pat) => name.span.merge(pat.span),
                            None => name.span,
                        };
                        PayloadField { name, value, span }
                    })
                    .boxed();

                // A variant's payload can look like `.circle(r)`, `.parallelogram((b, h))`, or
                // `.square { l }`.
                let variant_payload = choice((
                    self.kind(TokenKind::OpenParen)
                        .ignore_then(pattern.clone())
                        .then(self.kind(TokenKind::CloseParen))
                        .map(|(inner, close_tok)| {
                            (Payload::Single(Box::new(inner)), close_tok.span)
                        }),
                    self.kind(TokenKind::OpenBrace)
                        .ignore_then(
                            payload_field
                                .separated_by(self.kind(TokenKind::Comma))
                                .allow_trailing()
                                .collect::<Vec<_>>(),
                        )
                        .then(self.kind(TokenKind::CloseBrace))
                        .map(|(fields, close_tok)| (Payload::Record(fields), close_tok.span)),
                ))
                .boxed();

                // A leading `.` starts a variant pattern.
                let variant = self
                    .kind(TokenKind::Period)
                    .then(ident.clone())
                    .then(variant_payload.or_not())
                    .map(|((dot_tok, variant), payload)| {
                        let (payload, span) = match payload {
                            Some((payload, close_span)) => {
                                (payload, dot_tok.span.merge(close_span))
                            }
                            None => (Payload::None, dot_tok.span.merge(variant.span)),
                        };
                        Pattern {
                            kind: PatternKind::Variant { variant, payload },
                            span,
                        }
                    })
                    .boxed();

                let binding = ident
                    .clone()
                    .map(|name: Ident| Pattern {
                        kind: PatternKind::Binding(name),
                        span: name.span,
                    })
                    .boxed();

                choice((wildcard, literal, tuple, variant, binding))
            },
        )
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::interner::Interner;
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::lexer::Lexer;

    /// The single pattern a `Payload::Single` holds, or a panic.
    fn single(payload: &Payload<Pattern>) -> &Pattern {
        match payload {
            Payload::Single(inner) => inner,
            other => panic!("expected a single payload, got {other:?}"),
        }
    }

    fn record(payload: &Payload<Pattern>) -> &[PayloadField<Pattern>] {
        match payload {
            Payload::Record(fields) => fields,
            other => panic!("expected a record payload, got {other:?}"),
        }
    }

    fn parse_pattern(src: &str) -> Pattern {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
        let (output, errors) = parser
            .pattern_parser()
            .parse(&tokens[..])
            .into_output_errors();
        assert!(
            errors.is_empty(),
            "unexpected parse errors for {src:?}: {errors:?}"
        );
        output.expect("expected a successfully parsed pattern")
    }

    #[test]
    fn parses_wildcard_pattern() {
        let pat = parse_pattern("_");
        assert!(matches!(pat.kind, PatternKind::Wildcard));
    }

    #[test]
    fn parses_binding_pattern() {
        let pat = parse_pattern("x");
        match &pat.kind {
            PatternKind::Binding(name) => assert_eq!(Interner::resolve(name.text), "x"),
            other => panic!("expected a binding pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_int_literal_pattern() {
        let pat = parse_pattern("42");
        assert!(matches!(
            pat.kind,
            PatternKind::Literal(Literal::Int { .. })
        ));
    }

    #[test]
    fn parses_float_literal_pattern() {
        let pat = parse_pattern("1.618");
        assert!(matches!(
            pat.kind,
            PatternKind::Literal(Literal::Float { .. })
        ));
    }

    #[test]
    fn parses_string_literal_pattern() {
        let pat = parse_pattern(r#""hi""#);
        match &pat.kind {
            PatternKind::Literal(Literal::Str(sym)) => assert_eq!(Interner::resolve(*sym), "hi"),
            other => panic!("expected a string literal pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_char_literal_pattern() {
        let pat = parse_pattern("'a'");
        assert!(matches!(pat.kind, PatternKind::Literal(Literal::Char('a'))));
    }

    #[test]
    fn parses_bool_literal_patterns() {
        let pat = parse_pattern("true");
        assert!(matches!(
            pat.kind,
            PatternKind::Literal(Literal::Bool(true))
        ));

        let pat = parse_pattern("false");
        assert!(matches!(
            pat.kind,
            PatternKind::Literal(Literal::Bool(false))
        ));
    }

    #[test]
    fn parses_bare_variant_pattern() {
        let pat = parse_pattern(".rectangle");
        match &pat.kind {
            PatternKind::Variant { variant, payload } => {
                assert_eq!(Interner::resolve(variant.text), "rectangle");
                assert!(matches!(payload, Payload::None));
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_variant_pattern_with_single_payload() {
        let pat = parse_pattern(".circle(r)");
        match &pat.kind {
            PatternKind::Variant { variant, payload } => {
                assert_eq!(Interner::resolve(variant.text), "circle");
                match &single(payload).kind {
                    PatternKind::Binding(name) => assert_eq!(Interner::resolve(name.text), "r"),
                    other => panic!("expected a binding, got {other:?}"),
                }
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    /// A tuple payload is one value, so a tuple pattern nested inside the variant's single
    /// payload slot destructures it, not several comma-separated bindings.
    #[test]
    fn parses_variant_pattern_with_tuple_payload() {
        let pat = parse_pattern(".parallelogram((b, h))");
        match &pat.kind {
            PatternKind::Variant { variant, payload } => {
                assert_eq!(Interner::resolve(variant.text), "parallelogram");
                match &single(payload).kind {
                    PatternKind::Tuple(elems) => assert_eq!(elems.len(), 2),
                    other => panic!("expected a tuple pattern, got {other:?}"),
                }
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_variant_pattern_with_nested_payload() {
        let pat = parse_pattern(".some(.ok(x))");
        match &pat.kind {
            PatternKind::Variant { variant, payload } => {
                assert_eq!(Interner::resolve(variant.text), "some");
                match &single(payload).kind {
                    PatternKind::Variant { variant, .. } => {
                        assert_eq!(Interner::resolve(variant.text), "ok")
                    }
                    other => panic!("expected a nested variant pattern, got {other:?}"),
                }
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_variant_pattern_with_record_payload() {
        let pat = parse_pattern(".square { l: inner, w }");
        match &pat.kind {
            PatternKind::Variant { variant, payload } => {
                assert_eq!(Interner::resolve(variant.text), "square");
                let fields = record(payload);
                assert_eq!(fields.len(), 2);
                assert_eq!(Interner::resolve(fields[0].name.text), "l");
                match &fields[0].value.as_ref().expect("`l:` has a pattern").kind {
                    PatternKind::Binding(name) => assert_eq!(Interner::resolve(name.text), "inner"),
                    other => panic!("expected a binding, got {other:?}"),
                }
                // `w` is the field shorthand: no pattern of its own, it binds `w`.
                assert_eq!(Interner::resolve(fields[1].name.text), "w");
                assert!(fields[1].value.is_none());
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    /// The leading `.` is the only thing that makes a variant pattern, so a PascalCase bare
    /// identifier is a binding like any other. The old capitalization heuristic is gone.
    #[test]
    fn bare_pascal_case_identifier_is_a_binding() {
        let pat = parse_pattern("Rectangle");
        match &pat.kind {
            PatternKind::Binding(name) => assert_eq!(Interner::resolve(name.text), "Rectangle"),
            other => panic!("expected a binding pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_tuple_pattern() {
        let pat = parse_pattern("(x, y)");
        match &pat.kind {
            PatternKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[0].kind, PatternKind::Binding(_)));
                assert!(matches!(pats[1].kind, PatternKind::Binding(_)));
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tuple_pattern() {
        // `(a, (b, c))` exercises tuple nesting.
        let pat = parse_pattern("(a, (b, c))");
        match &pat.kind {
            PatternKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[0].kind, PatternKind::Binding(_)));
                match &pats[1].kind {
                    PatternKind::Tuple(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(inner[0].kind, PatternKind::Binding(_)));
                        assert!(matches!(inner[1].kind, PatternKind::Binding(_)));
                    }
                    other => panic!("expected a nested tuple pattern, got {other:?}"),
                }
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_tuple_pattern_with_variant_and_wildcard_elements() {
        // `(.circle(r), _)` exercises tuple + variant + wildcard nesting together.
        let pat = parse_pattern("(.circle(r), _)");
        match &pat.kind {
            PatternKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                match &pats[0].kind {
                    PatternKind::Variant { variant, payload } => {
                        assert_eq!(Interner::resolve(variant.text), "circle");
                        assert!(matches!(payload, Payload::Single(_)));
                    }
                    other => panic!("expected a variant pattern, got {other:?}"),
                }
                assert!(matches!(pats[1].kind, PatternKind::Wildcard));
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }
}
