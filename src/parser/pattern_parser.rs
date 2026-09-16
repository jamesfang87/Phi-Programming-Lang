use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{Ident, NodeId, Pat, PatKind, Payload, PayloadField};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl<'s> Parser<'s> {
    pub fn pattern_parser<'a>(&'a self) -> BoxedP<'a, Pat> {
        let ident = self.ident_parser();

        recursive(
            |pattern: Recursive<dyn ChumskyParser<'a, &'a [Token], Pat, Extra<'a>>>| {
                let wildcard = self
                    .kind(TokenKind::Wildcard)
                    .map(|t: Token| Pat::new(PatKind::Wildcard, t.span))
                    .boxed();

                let literal = self
                    .literal_parser()
                    .map(|(lit, span)| Pat::new(PatKind::Literal(lit), span))
                    .boxed();

                let single_element_tuple = self
                    .kind(TokenKind::OpenParen)
                    .then(pattern.clone())
                    .then_ignore(self.kind(TokenKind::Comma))
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((open_tok, element), close_tok)| {
                        Pat::new(
                            PatKind::Tuple(vec![element]),
                            open_tok.span.merge(close_tok.span),
                        )
                    })
                    .boxed();

                let tuple_or_group = self
                    .kind(TokenKind::OpenParen)
                    .then(
                        pattern
                            .clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((open_tok, mut pats), close_tok)| {
                        if pats.len() == 1 {
                            pats.pop().expect("checked len == 1 above")
                        } else {
                            Pat::new(PatKind::Tuple(pats), open_tok.span.merge(close_tok.span))
                        }
                    })
                    .boxed();

                let tuple = choice((single_element_tuple, tuple_or_group)).boxed();

                // `{ l }` binds the field to its own name
                // `{ l: <pat> }` allows for further destructuring with a nested pattern
                let payload_field = self
                    .ident_parser()
                    .then(
                        self.kind(TokenKind::Colon)
                            .ignore_then(pattern.clone())
                            .or_not(),
                    )
                    .map(|(name, value)| {
                        let span = match &value {
                            Some(value) => name.span.merge(value.span),
                            None => name.span,
                        };
                        PayloadField {
                            id: NodeId::next(),
                            name,
                            value,
                            span,
                        }
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
                        Pat::new(PatKind::Variant { variant, payload }, span)
                    })
                    .boxed();

                let binding = ident
                    .clone()
                    .map(|name: Ident| Pat::new(PatKind::Binding(name), name.span))
                    .boxed();

                // TODO: only wildcard/literal/tuple/variant/binding are parsed -- struct patterns
                // (`Point { x, y }`), `..` rest, and `|` or-patterns are missing, so
                // real destructuring beyond tuples/enums is rejected here (see `PatKind`).
                choice((wildcard, literal, tuple, variant, binding)).labelled("a pattern")
            },
        )
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Literal, PayloadField};
    use crate::testing::lex_src;

    /// The single pattern a `Payload::Single` holds, or a panic.
    fn single(payload: &Payload<Pat>) -> &Pat {
        match payload {
            Payload::Single(inner) => inner,
            other => panic!("expected a single payload, got {other:?}"),
        }
    }

    fn record(payload: &Payload<Pat>) -> &[PayloadField<Pat>] {
        match payload {
            Payload::Record(fields) => fields,
            other => panic!("expected a record payload, got {other:?}"),
        }
    }

    fn parse_pattern(src: &str) -> Pat {
        let (tokens, _) = lex_src(src);
        let parser = Parser::new(crate::testing::session());
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
        assert!(matches!(pat.kind, PatKind::Wildcard));
    }

    #[test]
    fn parses_binding_pattern() {
        let pat = parse_pattern("x");
        match &pat.kind {
            PatKind::Binding(name) => assert_eq!(crate::testing::resolve(name.text), "x"),
            other => panic!("expected a binding pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_int_literal_pattern() {
        let pat = parse_pattern("42");
        assert!(matches!(pat.kind, PatKind::Literal(Literal::Int { .. })));
    }

    #[test]
    fn parses_float_literal_pattern() {
        let pat = parse_pattern("1.618");
        assert!(matches!(pat.kind, PatKind::Literal(Literal::Float { .. })));
    }

    #[test]
    fn parses_string_literal_pattern() {
        let pat = parse_pattern(r#""hi""#);
        match &pat.kind {
            PatKind::Literal(Literal::Str(sym)) => assert_eq!(crate::testing::resolve(*sym), "hi"),
            other => panic!("expected a string literal pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_char_literal_pattern() {
        let pat = parse_pattern("'a'");
        assert!(matches!(pat.kind, PatKind::Literal(Literal::Char('a'))));
    }

    #[test]
    fn parses_bool_literal_patterns() {
        let pat = parse_pattern("true");
        assert!(matches!(pat.kind, PatKind::Literal(Literal::Bool(true))));

        let pat = parse_pattern("false");
        assert!(matches!(pat.kind, PatKind::Literal(Literal::Bool(false))));
    }

    #[test]
    fn parses_bare_variant_pattern() {
        let pat = parse_pattern(".rectangle");
        match &pat.kind {
            PatKind::Variant { variant, payload } => {
                assert_eq!(crate::testing::resolve(variant.text), "rectangle");
                assert!(matches!(payload, Payload::None));
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_variant_pattern_with_single_payload() {
        let pat = parse_pattern(".circle(r)");
        match &pat.kind {
            PatKind::Variant { variant, payload } => {
                assert_eq!(crate::testing::resolve(variant.text), "circle");
                match &single(payload).kind {
                    PatKind::Binding(name) => assert_eq!(crate::testing::resolve(name.text), "r"),
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
            PatKind::Variant { variant, payload } => {
                assert_eq!(crate::testing::resolve(variant.text), "parallelogram");
                match &single(payload).kind {
                    PatKind::Tuple(elems) => assert_eq!(elems.len(), 2),
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
            PatKind::Variant { variant, payload } => {
                assert_eq!(crate::testing::resolve(variant.text), "some");
                match &single(payload).kind {
                    PatKind::Variant { variant, .. } => {
                        assert_eq!(crate::testing::resolve(variant.text), "ok")
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
            PatKind::Variant { variant, payload } => {
                assert_eq!(crate::testing::resolve(variant.text), "square");
                let fields = record(payload);
                assert_eq!(fields.len(), 2);
                assert_eq!(crate::testing::resolve(fields[0].name.text), "l");
                match &fields[0].value.as_ref().expect("`l:` has a pattern").kind {
                    PatKind::Binding(name) => {
                        assert_eq!(crate::testing::resolve(name.text), "inner")
                    }
                    other => panic!("expected a binding, got {other:?}"),
                }
                // `w` is the field shorthand: no pattern of its own, it binds `w`.
                assert_eq!(crate::testing::resolve(fields[1].name.text), "w");
                assert!(fields[1].value.is_none());
            }
            other => panic!("expected a variant pattern, got {other:?}"),
        }
    }

    /// The leading `.` is the only thing that makes a variant pattern
    #[test]
    fn bare_pascal_case_identifier_is_a_binding() {
        let pat = parse_pattern("Rectangle");
        match &pat.kind {
            PatKind::Binding(name) => assert_eq!(crate::testing::resolve(name.text), "Rectangle"),
            other => panic!("expected a binding pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_tuple_pattern() {
        let pat = parse_pattern("(x, y)");
        match &pat.kind {
            PatKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[0].kind, PatKind::Binding(_)));
                assert!(matches!(pats[1].kind, PatKind::Binding(_)));
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tuple_pattern() {
        // `(a, (b, c))` exercises tuple nesting.
        let pat = parse_pattern("(a, (b, c))");
        match &pat.kind {
            PatKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                assert!(matches!(pats[0].kind, PatKind::Binding(_)));
                match &pats[1].kind {
                    PatKind::Tuple(inner) => {
                        assert_eq!(inner.len(), 2);
                        assert!(matches!(inner[0].kind, PatKind::Binding(_)));
                        assert!(matches!(inner[1].kind, PatKind::Binding(_)));
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
            PatKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                match &pats[0].kind {
                    PatKind::Variant { variant, payload } => {
                        assert_eq!(crate::testing::resolve(variant.text), "circle");
                        assert!(matches!(payload, Payload::Single(_)));
                    }
                    other => panic!("expected a variant pattern, got {other:?}"),
                }
                assert!(matches!(pats[1].kind, PatKind::Wildcard));
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }

    /// BUG: the tuple-pattern parser builds `PatKind::Tuple` unconditionally, so `(x)` becomes
    /// the one-element tuple pattern `(x,)` instead of a parenthesized binding. The expression
    /// parser unwraps `(e)` to `e`; the pattern grammar should do the same, reserving `(p,)` for
    /// the one-element tuple.
    ///
    /// Run with `cargo test --bin phi -- --ignored` to reproduce.
    #[test]
    fn parses_parenthesized_pattern_as_the_inner_pattern() {
        let pat = parse_pattern("(x)");
        assert!(
            matches!(pat.kind, PatKind::Binding(_)),
            "expected `(x)` to parse as a binding, got {pat:?}"
        );
    }
}
