use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{Expr, ExprKind, Literal, Pattern, PatternKind};
use crate::interner::Interner;

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl Parser {
    pub fn pattern_parser<'a>(&'a self) -> BoxedP<'a, Pattern> {
        let ident = self.ident_parser();
        let path = self.path_parser();

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

                // `Circle(r)`, `Parallelogram(b, h)` — a ctor payload is a parenthesized list of
                // bindings, distinct from `tuple` above only in that it always follows a path.
                let ctor_payload = self
                    .kind(TokenKind::OpenParen)
                    .ignore_then(
                        ident
                            .clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|(payload, close_tok)| (payload, close_tok.span))
                    .boxed();

                // A single bare identifier is ambiguous between a binding (`x`) and a unit ctor
                // pattern (`Rectangle`) at the grammar level, so — matching the PascalCase
                // convention called out on `PatternKind::Ctor` — we resolve it by the identifier's
                // first character: lowercase/underscore is a binding, uppercase is a ctor. A
                // qualified path (`Shape::Circle`) or a path with a payload is always a ctor.
                let ctor_or_binding = path
                    .clone()
                    .then(ctor_payload.or_not())
                    .map(|(path, payload)| {
                        let starts_uppercase = Interner::resolve(path.segments[0].text)
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_uppercase());

                        if path.segments.len() == 1 && !starts_uppercase && payload.is_none() {
                            let name = path.segments[0];
                            return Pattern {
                                kind: PatternKind::Binding(name),
                                span: name.span,
                            };
                        }

                        let (payload, span) = match payload {
                            Some((payload, close_span)) => {
                                (payload, path.span.merge(close_span))
                            }
                            None => (Vec::new(), path.span),
                        };

                        Pattern {
                            kind: PatternKind::Ctor { path, payload },
                            span,
                        }
                    })
                    .boxed();

                choice((wildcard, literal, tuple, ctor_or_binding))
            },
        )
        .boxed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::lexer::Lexer;

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
        assert!(matches!(
            pat.kind,
            PatternKind::Literal(Literal::Char('a'))
        ));
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
    fn parses_bare_ctor_pattern() {
        let pat = parse_pattern("Rectangle");
        match &pat.kind {
            PatternKind::Ctor { path, payload } => {
                assert_eq!(path.segments.len(), 1);
                assert_eq!(Interner::resolve(path.segments[0].text), "Rectangle");
                assert!(payload.is_empty());
            }
            other => panic!("expected a ctor pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_ctor_pattern_with_single_payload() {
        let pat = parse_pattern("Circle(r)");
        match &pat.kind {
            PatternKind::Ctor { path, payload } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Circle");
                assert_eq!(payload.len(), 1);
                assert_eq!(Interner::resolve(payload[0].text), "r");
            }
            other => panic!("expected a ctor pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_ctor_pattern_with_multiple_payload() {
        let pat = parse_pattern("Parallelogram(b, h)");
        match &pat.kind {
            PatternKind::Ctor { path, payload } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Parallelogram");
                assert_eq!(payload.len(), 2);
                assert_eq!(Interner::resolve(payload[0].text), "b");
                assert_eq!(Interner::resolve(payload[1].text), "h");
            }
            other => panic!("expected a ctor pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_qualified_ctor_pattern() {
        let pat = parse_pattern("Shape::Circle");
        match &pat.kind {
            PatternKind::Ctor { path, payload } => {
                assert_eq!(path.segments.len(), 2);
                assert_eq!(Interner::resolve(path.segments[0].text), "Shape");
                assert_eq!(Interner::resolve(path.segments[1].text), "Circle");
                assert!(payload.is_empty());
            }
            other => panic!("expected a ctor pattern, got {other:?}"),
        }
    }

    #[test]
    fn parses_qualified_ctor_pattern_with_payload() {
        let pat = parse_pattern("Shape::Circle(r)");
        match &pat.kind {
            PatternKind::Ctor { path, payload } => {
                assert_eq!(path.segments.len(), 2);
                assert_eq!(payload.len(), 1);
                assert_eq!(Interner::resolve(payload[0].text), "r");
            }
            other => panic!("expected a ctor pattern, got {other:?}"),
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
    fn parses_tuple_pattern_with_ctor_and_wildcard_elements() {
        // `(Circle(r), _)` exercises tuple + ctor + wildcard nesting together.
        let pat = parse_pattern("(Circle(r), _)");
        match &pat.kind {
            PatternKind::Tuple(pats) => {
                assert_eq!(pats.len(), 2);
                match &pats[0].kind {
                    PatternKind::Ctor { path, payload } => {
                        assert_eq!(Interner::resolve(path.segments[0].text), "Circle");
                        assert_eq!(payload.len(), 1);
                    }
                    other => panic!("expected a ctor pattern, got {other:?}"),
                }
                assert!(matches!(pats[1].kind, PatternKind::Wildcard));
            }
            other => panic!("expected a tuple pattern, got {other:?}"),
        }
    }
}
