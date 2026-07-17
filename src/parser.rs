//! A chumsky-based parser over the lexer's token stream.

use chumsky::Parser as ChumskyParser;
use chumsky::error::Rich;
use chumsky::extra;
use chumsky::prelude::*;

use crate::ast::interner::Interner;
use crate::ast::{BinaryOp, Expr, ExprKind, Ident, Item, ItemKind, Path, SrcUnit};
use crate::diag::DiagCtx;
use crate::driver::src_map::SrcMap;
use crate::lexer::src_span::SrcSpan;
use crate::lexer::token::{Token, TokenKind};

type Extra<'a> = extra::Err<Rich<'a, Token>>;
type BoxedP<'a, O> = Boxed<'a, 'a, &'a [Token], O, Extra<'a>>;

mod block_parser;
mod expr_parser;
mod item_parser;
mod pattern_parser;
mod type_parser;

pub struct Parser {
    tokens: Vec<Token>,
    file_offset: usize,
}

impl Parser {
    pub fn new(tokens: Vec<Token>, file_offset: usize) -> Self {
        Parser {
            tokens,
            file_offset,
        }
    }

    pub fn parse(&self) -> SrcUnit {
        let (output, errors) = self.grammar().parse(&self.tokens[..]).into_output_errors();

        for err in &errors {
            self.report_error(err);
        }

        output.unwrap_or_else(|| SrcUnit {
            module: None,
            imports: Vec::new(),
            items: Vec::new(),
            span: SrcSpan::new(0, 0),
        })
    }

    fn report_error(&self, err: &Rich<Token>) {
        let span = err
            .found()
            .map(|t| t.span)
            .unwrap_or_else(|| SrcSpan::new(0, 0));
        DiagCtx::error(err.to_string(), span);
    }

    /// Matches a single token of the given `kind`, yielding the token itself.
    fn kind<'a>(
        &'a self,
        k: TokenKind,
    ) -> impl ChumskyParser<'a, &'a [Token], Token, Extra<'a>> + Clone {
        any().filter(move |t: &Token| t.kind == k)
    }

    fn ident_parser<'a>(&'a self) -> BoxedP<'a, Ident> {
        self.kind(TokenKind::Identifier)
            .map(|t: Token| Ident {
                text: Interner::intern(
                    &SrcMap::text_of(t.span)
                        .expect("lexer token span should always resolve to a source file"),
                ),
                span: t.span,
            })
            .boxed()
    }

    fn path_parser<'a>(&'a self) -> BoxedP<'a, Path> {
        self.ident_parser()
            .separated_by(self.kind(TokenKind::DoubleColon))
            .at_least(1)
            .collect::<Vec<_>>()
            .map(|segments: Vec<Ident>| {
                let span = segments[0].span.merge(segments[segments.len() - 1].span);
                Path { segments, span }
            })
            .boxed()
    }

    /// Builds an error-recovery parser: skip at least one token, then keep skipping until the
    /// next token that looks like it could start a new instance of whatever just failed to
    /// parse (`boundary`) — or until input runs out — then produce `fallback`.
    ///
    /// Used via `some_parser.recover_with(via_parser(self.recover_to_boundary(...)))`.
    ///
    /// The mandatory first skip matters: every well-formed construct we recover for begins with
    /// a `boundary` token, so the position where the primary parser just failed always matches
    /// `boundary` too. Without forcing at least one token of progress, "recovery" would succeed
    /// without consuming anything, and `.repeated()` around it would loop forever — chumsky's
    /// no-progress guard turns that into a panic rather than a hang, but either way it's wrong.
    fn recover_to_boundary<'a, O: Clone + 'a>(
        &'a self,
        boundary: impl ChumskyParser<'a, &'a [Token], (), Extra<'a>> + Clone + 'a,
        fallback: O,
    ) -> impl ChumskyParser<'a, &'a [Token], O, Extra<'a>> + Clone + 'a {
        any()
            .ignored()
            .then(any().and_is(boundary.not()).ignored().repeated())
            .to(fallback)
    }

    /// Builds the whole grammar for a single file: a sequence of items followed by end-of-input.
    ///
    /// A malformed item doesn't take the rest of the file down with it: if `item_parser()` fails,
    /// recovery skips tokens up to the next token that plausibly starts a new item (or to
    /// end-of-input) and yields an `ItemKind::Error` in its place, so parsing resumes there and
    /// every other item in the file still gets reported.
    fn grammar<'a>(&'a self) -> impl ChumskyParser<'a, &'a [Token], SrcUnit, Extra<'a>> + Clone {
        let item_start = choice((
            self.kind(TokenKind::PublicKw).ignored(),
            self.kind(TokenKind::FunKw).ignored(),
            self.kind(TokenKind::StructKw).ignored(),
            self.kind(TokenKind::EnumKw).ignored(),
            self.kind(TokenKind::TraitKw).ignored(),
            self.kind(TokenKind::ExtendKw).ignored(),
            self.kind(TokenKind::ModuleKw).ignored(),
            self.kind(TokenKind::ImportKw).ignored(),
        ));

        let item = self
            .item_parser()
            .recover_with(via_parser(self.recover_to_boundary(
                item_start,
                Item {
                    kind: ItemKind::Error,
                    span: SrcSpan::new(0, 0),
                },
            )));

        item.repeated()
            .collect::<Vec<_>>()
            .then(end())
            .map(|(items, _)| {
                let span = match (items.first(), items.last()) {
                    (Some(first), Some(last)) => first.span.merge(last.span),
                    _ => SrcSpan::new(self.file_offset, self.file_offset),
                };
                SrcUnit {
                    module: None,
                    imports: Vec::new(),
                    items,
                    span,
                }
            })
    }
}

fn combine_binary(lhs: Expr, ((op, _op_span), rhs): ((BinaryOp, SrcSpan), Expr)) -> Expr {
    let span = lhs.span.merge(rhs.span);
    Expr {
        kind: ExprKind::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        },
        span,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::diag::DiagCtx;
    use crate::lexer::Lexer;

    /// Lexes and parses `src`, asserting there were no diagnostics along the way.
    fn parse_ok(src: &str) -> SrcUnit {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let unit = Parser::new(tokens, offset).parse();
        let diagnostics = DiagCtx::diagnostics();
        assert!(
            diagnostics.is_empty(),
            "unexpected diagnostics for {src:?}: {diagnostics:?}"
        );
        unit
    }

    /// Lexes and parses `src`, returning how many diagnostics were raised (without asserting
    /// they're empty, unlike [`parse_ok`]).
    fn diagnostic_count(src: &str) -> usize {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let _ = Parser::new(tokens, offset).parse();
        DiagCtx::diagnostics().len()
    }

    /// Like [`diagnostic_count`], but also returns the (best-effort, possibly error-containing)
    /// parsed unit — for exercising recovery.
    fn parse_with_errors(src: &str) -> (SrcUnit, usize) {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let unit = Parser::new(tokens, offset).parse();
        (unit, DiagCtx::diagnostics().len())
    }

    fn text(ident: Ident) -> String {
        Interner::resolve(ident.text)
    }

    fn only_function(unit: &SrcUnit) -> &Function {
        assert_eq!(unit.items.len(), 1);
        match &unit.items[0].kind {
            ItemKind::Function(f) => f,
            other => panic!("expected a single function item, got {other:?}"),
        }
    }

    #[test]
    fn parses_empty_function() {
        let unit = parse_ok("fun main() {}");
        let f = only_function(&unit);
        assert_eq!(text(f.name), "main");
        assert!(matches!(f.visibility, Visibility::Private));
        assert!(f.params.is_empty());
        assert!(f.ret.is_none());
        assert_eq!(f.body.as_ref().unwrap().stmts.len(), 0);
    }

    #[test]
    fn parses_public_function_with_params_and_return_type() {
        let unit = parse_ok("public fun add(x: i32, y: i32) -> i32 { return x + y; }");
        let f = only_function(&unit);
        assert!(matches!(f.visibility, Visibility::Public));
        assert_eq!(f.params.len(), 2);
        assert_eq!(text(f.params[0].name), "x");
        assert_eq!(text(f.params[1].name), "y");
        for param in &f.params {
            match &param.ty.kind {
                Ty::Base { base, args } => {
                    assert_eq!(text(base.segments[0]), "i32");
                    assert!(args.is_empty());
                }
                other => panic!("expected a base type, got {other:?}"),
            }
        }
        match &f.ret.as_ref().unwrap().kind {
            Ty::Base { base, .. } => assert_eq!(text(base.segments[0]), "i32"),
            other => panic!("expected a base type, got {other:?}"),
        }

        let body = f.body.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 1);
        match &body.stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Binary { op, lhs, rhs } => {
                    assert_eq!(*op, BinaryOp::Add);
                    assert!(matches!(lhs.kind, ExprKind::DeclRef(_)));
                    assert!(matches!(rhs.kind, ExprKind::DeclRef(_)));
                }
                other => panic!("expected a binary expr, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_call_with_string_literal_argument() {
        let unit = parse_ok(r#"fun main() { println("Hello, world!"); }"#);
        let f = only_function(&unit);
        let body = f.body.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 1);
        match &body.stmts[0].kind {
            StmtKind::Expr(expr) => match &expr.kind {
                ExprKind::FunCall { callee, args } => {
                    match &callee.kind {
                        ExprKind::DeclRef(path) => assert_eq!(text(path.segments[0]), "println"),
                        other => panic!("expected a decl-ref callee, got {other:?}"),
                    }
                    assert_eq!(args.len(), 1);
                    match &args[0].kind {
                        ExprKind::Literal(Literal::Str(sym)) => {
                            assert_eq!(Interner::resolve(*sym), "Hello, world!")
                        }
                        other => panic!("expected a string literal, got {other:?}"),
                    }
                }
                other => panic!("expected a call expr, got {other:?}"),
            },
            other => panic!("expected an expression statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_let_with_mut_and_type_annotation() {
        let unit = parse_ok("fun main() { let mut phi: f64 = 1.618; }");
        let f = only_function(&unit);
        let body = f.body.as_ref().unwrap();
        match &body.stmts[0].kind {
            StmtKind::Decl(DeclStmt {
                mutability,
                name,
                ty,
                expr,
                ..
            }) => {
                assert!(matches!(mutability, Mutability::Mutable));
                match &name.kind {
                    PatternKind::Binding(name) => assert_eq!(text(*name), "phi"),
                    other => panic!("expected a binding pattern, got {other:?}"),
                }
                match &ty.as_ref().unwrap().kind {
                    Ty::Base { base, .. } => assert_eq!(text(base.segments[0]), "f64"),
                    other => panic!("expected a base type, got {other:?}"),
                }
                match &expr.kind {
                    ExprKind::Literal(Literal::Float { value, .. }) => {
                        assert_eq!(Interner::resolve(*value), "1.618")
                    }
                    other => panic!("expected a float literal, got {other:?}"),
                }
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_immutable_let_without_type_annotation() {
        let unit = parse_ok("fun main() { let foo = 0; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Decl(DeclStmt { mutability, ty, .. }) => {
                assert!(matches!(mutability, Mutability::Immutable));
                assert!(ty.is_none());
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_while_loop() {
        let unit = parse_ok("fun main() { while i < 5 { foo(); } }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::While { cond, body } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Lt,
                        ..
                    }
                ));
                assert_eq!(body.stmts.len(), 1);
            }
            other => panic!("expected a while statement, got {other:?}"),
        }
    }

    #[test]
    fn respects_arithmetic_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3), not (1 + 2) * 3.
        let unit = parse_ok("fun main() { return 1 + 2 * 3; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    rhs,
                    ..
                } => {
                    assert!(matches!(
                        rhs.kind,
                        ExprKind::Binary {
                            op: BinaryOp::Mul,
                            ..
                        }
                    ));
                }
                other => panic!("expected a top-level `+`, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn parens_override_precedence() {
        // (1 + 2) * 3 should parse with `*` at the top.
        let unit = parse_ok("fun main() { return (1 + 2) * 3; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Binary {
                    op: BinaryOp::Mul,
                    lhs,
                    ..
                } => {
                    assert!(matches!(
                        lhs.kind,
                        ExprKind::Binary {
                            op: BinaryOp::Add,
                            ..
                        }
                    ));
                }
                other => panic!("expected a top-level `*`, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn unary_minus_binds_tighter_than_binary_operators() {
        let unit = parse_ok("fun main() { return -1 + 2; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Binary {
                    op: BinaryOp::Add,
                    lhs,
                    ..
                } => {
                    assert!(matches!(
                        lhs.kind,
                        ExprKind::Unary {
                            op: UnaryOp::Neg,
                            ..
                        }
                    ));
                }
                other => panic!("expected a top-level `+`, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn logical_operators_parse_with_and_binding_tighter_than_or() {
        let unit = parse_ok("fun main() { return true || false && true; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Binary {
                    op: BinaryOp::Or,
                    rhs,
                    ..
                } => {
                    assert!(matches!(
                        rhs.kind,
                        ExprKind::Binary {
                            op: BinaryOp::And,
                            ..
                        }
                    ));
                }
                other => panic!("expected a top-level `||`, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_functions() {
        let unit = parse_ok("fun a() {} fun b() {}");
        assert_eq!(unit.items.len(), 2);
        for item in &unit.items {
            assert!(matches!(item.kind, ItemKind::Function(_)));
        }
    }

    #[test]
    fn parses_char_and_bool_literals() {
        let unit = parse_ok("fun main() { return 'a'; }");
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => {
                assert!(matches!(expr.kind, ExprKind::Literal(Literal::Char('a'))));
            }
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn escape_sequences_are_unescaped_in_string_literals() {
        let unit = parse_ok(r#"fun main() { return "a\nb"; }"#);
        let f = only_function(&unit);
        match &f.body.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return { ret: expr } => match &expr.kind {
                ExprKind::Literal(Literal::Str(sym)) => {
                    assert_eq!(Interner::resolve(*sym), "a\nb")
                }
                other => panic!("expected a string literal, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn reports_diagnostic_on_missing_semicolon() {
        assert_eq!(diagnostic_count("fun main() { let x = 1 }"), 1);
    }

    #[test]
    fn reports_diagnostic_on_unclosed_brace() {
        assert_eq!(diagnostic_count("fun main() { return 1;"), 1);
    }

    #[test]
    fn recovers_from_a_malformed_item_and_keeps_parsing_later_items() {
        // `1 + 2;` isn't a valid item at all; the well-formed function after it should still
        // come through.
        let (unit, error_count) = parse_with_errors("1 + 2; fun ok() {}");
        assert_eq!(error_count, 1);
        assert_eq!(unit.items.len(), 2);
        assert!(matches!(unit.items[0].kind, ItemKind::Error));
        match &unit.items[1].kind {
            ItemKind::Function(f) => assert_eq!(text(f.name), "ok"),
            other => panic!("expected a function item, got {other:?}"),
        }
    }

    #[test]
    fn recovers_from_multiple_malformed_items() {
        let (unit, error_count) = parse_with_errors("fun a() {} ???; fun b() {} !!!; fun c() {}");
        assert_eq!(error_count, 2);
        assert_eq!(unit.items.len(), 5);
        let function_names: Vec<String> = unit
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Function(f) => Some(text(f.name)),
                _ => None,
            })
            .collect();
        assert_eq!(function_names, vec!["a", "b", "c"]);
    }
}
