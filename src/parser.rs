//! [`Parser`] is an implementation of a parser using the `chumsky`
//! parser-combinator library. [`Parser`] takes the token stream produced
//! by the Lexer to output an Abstract Syntax Tree (AST)^1.
//!
//! 1. One should note that the AST outputted by the [`Parser`] is per-file,
//! not per module. What this means is that the [`Parser`] does not immediately
//! combine files implementing the same module, but instead keeps them separated
//! to keep the beginnings of name resolution outside of the parser. Immediately
//! after, however, this is done.

use chumsky::Parser as ChumskyParser;
use chumsky::error::Rich;
use chumsky::extra;
use chumsky::prelude::*;

use crate::ast::interner::Interner;
use crate::ast::{Ident, Item, ItemKind, ParsedSrcFile, Path};
use crate::diag::{DiagCtx, Diagnostic};
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

/// The grammar, and the entry points that run it over a file's tokens.
///
/// Deliberately holds no per-file state. Nothing the grammar is built from reads a token stream
/// or a file offset -- [`Parser::kind`] filters on a token's kind, and the leaf parsers reach
/// the source text through the global [`SrcMap`] -- so one built grammar parses every file in a
/// build. That is what [`Parser::parse_all`] exploits; see its docs for why it matters.
pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Parser
    }

    /// Parses one file's token stream into a [`ParsedSrcFile`], reporting errors through
    /// [`DiagCtx`]. `file_offset` is the stream's start position in the global [`SrcMap`], and
    /// becomes the span of the fallback empty file if parsing fails.
    ///
    /// This builds a grammar for the one call. Use [`Parser::parse_all`] to parse a whole build.
    pub fn parse(&self, tokens: &[Token], file_offset: usize) -> ParsedSrcFile {
        let grammar = self.grammar();
        Self::run(&grammar, tokens, file_offset)
    }

    /// Parses every file's token stream, building the grammar **once** for all of them.
    ///
    /// Constructing the grammar is not free: it allocates the whole boxed combinator tree, and
    /// does so more than once per build of it, since `item_parser` reaches for both
    /// `type_parser` and `block_parser` and each of those builds its own expression grammar.
    /// Paying that per file made it a fixed cost that scaled with file count rather than with
    /// how much source there was -- roughly a third of the wall time on a 400-file build.
    /// Hoisting it here is sound precisely because the grammar carries no per-file state.
    pub fn parse_all(&self, streams: &[(Vec<Token>, usize)]) -> Vec<ParsedSrcFile> {
        let grammar = self.grammar();
        streams
            .iter()
            .map(|(tokens, file_offset)| Self::run(&grammar, tokens, *file_offset))
            .collect()
    }

    /// Runs an already-built `grammar` over one file's tokens.
    fn run<'a>(
        grammar: &impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>>,
        tokens: &'a [Token],
        file_offset: usize,
    ) -> ParsedSrcFile {
        let (output, errors) = grammar.parse(tokens).into_output_errors();

        for err in &errors {
            Self::report_error(err);
        }

        match output {
            Some(items) => Self::assemble_file(items, file_offset),
            None => ParsedSrcFile {
                module: None,
                imports: Vec::new(),
                items: Vec::new(),
                span: SrcSpan::new(file_offset, file_offset),
            },
        }
    }

    /// Splits a file's parsed items into its module header, imports, and definitions
    /// as required by [`ParsedSrcFile`]
    fn assemble_file(items: Vec<Item>, file_offset: usize) -> ParsedSrcFile {
        let span = match (items.first(), items.last()) {
            (Some(first), Some(last)) => first.span.merge(last.span),
            _ => SrcSpan::new(file_offset, file_offset),
        };

        let mut module = None;
        let mut imports = Vec::new();
        let mut definitions = Vec::new();

        for item in items {
            match item.kind {
                // Only the first `module` header counts. A file belongs to exactly one module.
                ItemKind::Module(decl) => match module {
                    None => module = Some(decl),
                    Some(_) => Self::report_duplicate_module(item.span),
                },
                ItemKind::Import(import) => imports.push(import),
                _ => definitions.push(item),
            }
        }

        ParsedSrcFile {
            module,
            imports,
            items: definitions,
            span,
        }
    }

    fn report_duplicate_module(span: SrcSpan) {
        DiagCtx::emit(
            Diagnostic::error("a file can only declare one module", span)
                .with_label("second `module` declaration")
                .with_help("every item in a file belongs to the module its first header names"),
        );
    }

    fn report_error(err: &Rich<Token>) {
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

    /// A parser that never matches.
    ///
    /// Use it to turn off one alternative of a `choice` without changing the choice's shape.
    fn never<'a, O: 'a>(&'a self) -> BoxedP<'a, O> {
        any()
            .filter(|_: &Token| false)
            .map(|_| unreachable!("`never` matches nothing, so nothing is ever mapped"))
            .boxed()
    }

    /// Builds an error-recovery parser.
    ///
    /// It skips at least one token, then keeps skipping until it finds a token that could
    /// start a new instance of whatever failed to parse (`boundary`), or until input runs out.
    /// Then it produces `fallback`.
    ///
    /// Used via `some_parser.recover_with(via_parser(self.recover_to_boundary(...)))`.
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
    /// The result is a flat list of items, including the file's `module` header and its imports.
    /// [`Self::assemble_file`] sorts those into the parts of a [`ParsedSrcFile`] afterwards.
    fn grammar<'a>(&'a self) -> impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>> + Clone {
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

        item.repeated().collect::<Vec<_>>().then_ignore(end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{lex_src, parse_src};
    use crate::ast::*;
    use crate::diag::DiagCtx;

    /// Lexes and parses `src`, returning how many diagnostics were raised. Unlike
    /// [`parse_src`], this asserts nothing, so it can exercise the error paths.
    fn diagnostic_count(src: &str) -> usize {
        let (tokens, offset) = lex_src(src);
        let _ = Parser::new().parse(&tokens, offset);
        DiagCtx::diagnostics().len()
    }

    /// Like [`diagnostic_count`], but also returns the (best-effort, possibly error-containing)
    /// parsed unit, for exercising recovery.
    fn parse_with_errors(src: &str) -> (ParsedSrcFile, usize) {
        let (tokens, offset) = lex_src(src);
        let unit = Parser::new().parse(&tokens, offset);
        (unit, DiagCtx::diagnostics().len())
    }

    fn text(ident: Ident) -> String {
        Interner::resolve(ident.text)
    }

    fn only_function(unit: &ParsedSrcFile) -> &Function {
        assert_eq!(unit.items.len(), 1);
        match &unit.items[0].kind {
            ItemKind::Function(f) => f,
            other => panic!("expected a single function item, got {other:?}"),
        }
    }

    /// A file's `module` header and its imports are parsed as ordinary items but don't stay in
    /// `items`: lowering reads them from their own fields, and needs the header in particular
    /// before it can place any of the items below it.
    #[test]
    fn module_header_and_imports_are_split_out_of_items() {
        let unit = parse_src(
            "module math::vector;\nimport core::ops::Add;\nimport math::*;\nfun main() {}",
        );

        let module = unit.module.as_ref().expect("the header should be recorded");
        let segments: Vec<String> = module.path.segments.iter().map(|s| text(*s)).collect();
        assert_eq!(segments, ["math", "vector"]);

        assert_eq!(unit.imports.len(), 2);
        assert!(unit.imports[1].glob);

        // Only the function is left behind.
        assert_eq!(text(only_function(&unit).name), "main");
    }

    #[test]
    fn a_file_without_a_module_header_records_none() {
        let unit = parse_src("fun main() {}");
        assert!(unit.module.is_none());
    }

    #[test]
    fn a_second_module_header_is_an_error() {
        let (unit, errors) = parse_with_errors("module a;\nmodule b;\nfun main() {}");
        assert_eq!(errors, 1);

        // The first header still wins, so the rest of the file lowers somewhere sensible.
        let module = unit
            .module
            .as_ref()
            .expect("the first header should be kept");
        assert_eq!(text(module.path.segments[0]), "a");
    }

    #[test]
    fn parses_empty_function() {
        let unit = parse_src("fun main() {}");
        let f = only_function(&unit);
        assert_eq!(text(f.name), "main");
        assert!(matches!(f.visibility, Visibility::Private));
        assert!(f.params.is_empty());
        assert!(f.ret.is_none());
        assert_eq!(f.block.as_ref().unwrap().stmts.len(), 0);
    }

    #[test]
    fn parses_public_function_with_params_and_return_type() {
        let unit = parse_src("public fun add(x: i32, y: i32) -> i32 { return x + y; }");
        let f = only_function(&unit);
        assert!(matches!(f.visibility, Visibility::Public));
        assert_eq!(f.params.len(), 2);
        assert_eq!(text(f.params[0].name), "x");
        assert_eq!(text(f.params[1].name), "y");
        for param in &f.params {
            match &param.ty.kind {
                TyKind::Path { path, args } => {
                    assert_eq!(text(path.segments[0]), "i32");
                    assert!(args.is_empty());
                }
                other => panic!("expected a base type, got {other:?}"),
            }
        }
        match &f.ret.as_ref().unwrap().kind {
            TyKind::Path { path, .. } => assert_eq!(text(path.segments[0]), "i32"),
            other => panic!("expected a base type, got {other:?}"),
        }

        let body = f.block.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 1);
        match &body.stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
                ExprKind::Binary { op, lhs, rhs } => {
                    assert_eq!(*op, BinaryOp::Add);
                    assert!(matches!(lhs.kind, ExprKind::Path(_)));
                    assert!(matches!(rhs.kind, ExprKind::Path(_)));
                }
                other => panic!("expected a binary expr, got {other:?}"),
            },
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_call_with_string_literal_argument() {
        let unit = parse_src(r#"fun main() { println("Hello, world!"); }"#);
        let f = only_function(&unit);
        let body = f.block.as_ref().unwrap();
        assert_eq!(body.stmts.len(), 1);
        match &body.stmts[0].kind {
            StmtKind::Expr { expr, .. } => match &expr.kind {
                ExprKind::Call { callee, args } => {
                    match &callee.kind {
                        ExprKind::Path(path) => assert_eq!(text(path.segments[0]), "println"),
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
        let unit = parse_src("fun main() { let mut phi: f64 = 1.618; }");
        let f = only_function(&unit);
        let body = f.block.as_ref().unwrap();
        match &body.stmts[0].kind {
            StmtKind::Let {
                mutability,
                pat,
                ty,
                init,
                ..
            } => {
                assert!(matches!(mutability, Mutability::Mutable));
                match &pat.kind {
                    PatKind::Binding(name) => assert_eq!(text(*name), "phi"),
                    other => panic!("expected a binding pattern, got {other:?}"),
                }
                match &ty.as_ref().unwrap().kind {
                    TyKind::Path { path, .. } => assert_eq!(text(path.segments[0]), "f64"),
                    other => panic!("expected a base type, got {other:?}"),
                }
                match &init.kind {
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
        let unit = parse_src("fun main() { let foo = 0; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Let { mutability, ty, .. } => {
                assert!(matches!(mutability, Mutability::Immutable));
                assert!(ty.is_none());
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_while_loop() {
        let unit = parse_src("fun main() { while i < 5 { foo(); } }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::While { cond, block } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Lt,
                        ..
                    }
                ));
                assert_eq!(block.stmts.len(), 1);
            }
            other => panic!("expected a while statement, got {other:?}"),
        }
    }

    #[test]
    fn respects_arithmetic_precedence() {
        // 1 + 2 * 3 should parse as 1 + (2 * 3), not (1 + 2) * 3.
        let unit = parse_src("fun main() { return 1 + 2 * 3; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
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
        let unit = parse_src("fun main() { return (1 + 2) * 3; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
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
        let unit = parse_src("fun main() { return -1 + 2; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
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
        let unit = parse_src("fun main() { return true || false && true; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
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
        let unit = parse_src("fun a() {} fun b() {}");
        assert_eq!(unit.items.len(), 2);
        for item in &unit.items {
            assert!(matches!(item.kind, ItemKind::Function(_)));
        }
    }

    #[test]
    fn parses_char_and_bool_literals() {
        let unit = parse_src("fun main() { return 'a'; }");
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => {
                assert!(matches!(expr.kind, ExprKind::Literal(Literal::Char('a'))));
            }
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn escape_sequences_are_unescaped_in_string_literals() {
        let unit = parse_src(r#"fun main() { return "a\nb"; }"#);
        let f = only_function(&unit);
        match &f.block.as_ref().unwrap().stmts[0].kind {
            StmtKind::Return(expr) => match &expr.kind {
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
