use chumsky::Parser as ChumskyParser;
use chumsky::error::Rich;
use chumsky::extra;
use chumsky::input::InputRef;
use chumsky::prelude::*;

use crate::ast::interner::Interner;
use crate::ast::{Ast, Ident, Item, ItemKind, NodeId, ParsedSrcFile, Path};
use crate::diagnostics::DiagCtx;
use crate::diagnostics::parser::{report_duplicate_module, report_errors};
use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::token::{Token, TokenKind};

type Extra<'a> = extra::Err<Rich<'a, Token>>;
type BoxedP<'a, O> = Boxed<'a, 'a, &'a [Token], O, Extra<'a>>;

mod block_parser;
mod delimiters;
mod expr_parser;
mod item_parser;
mod pattern_parser;
mod type_parser;

/// Whether [`Parser::recover_by_skipping`] leaves a token for the next parser or consumes it.
#[derive(Clone, Copy)]
enum Stop {
    /// Leave the token unconsumed: it starts the next construct.
    Before,
    /// Consume the token: it belongs to the construct being skipped, as a `;` does to the
    /// statement it terminates.
    After,
}

/// Maps a token to where item recovery stops, for `Parser::grammar`.
///
/// The keywords listed are the ones `Parser::item_parser` accepts as an item's first token.
fn item_recovery_point(kind: TokenKind) -> Option<Stop> {
    matches!(
        kind,
        TokenKind::PublicKw
            | TokenKind::FunKw
            | TokenKind::StructKw
            | TokenKind::EnumKw
            | TokenKind::TraitKw
            | TokenKind::ExtendKw
            | TokenKind::ModuleKw
            | TokenKind::ImportKw
    )
    .then_some(Stop::Before)
}

pub struct Parser {}

impl Parser {
    pub fn new() -> Self {
        Parser {}
    }

    pub fn parse(&self, tokens: &[Token], file_offset: usize) -> ParsedSrcFile {
        let grammar = self.grammar();
        Self::run(&grammar, tokens, file_offset)
    }

    pub fn parse_all(&self, streams: &[(Vec<Token>, usize)]) -> Ast {
        let grammar = self.grammar();
        let files = streams
            .iter()
            .map(|(tokens, file_offset)| Self::run(&grammar, tokens, *file_offset))
            .collect();
        Ast::new(files)
    }

    fn run<'a>(
        grammar: &impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>>,
        tokens: &'a [Token],
        file_offset: usize,
    ) -> ParsedSrcFile {
        let (output, errors) = grammar.parse(tokens).into_output_errors();

        // Every failure after an unmatched delimiter is a consequence of it, so the two reports
        // are alternatives rather than both being emitted.
        let unmatched = delimiters::unmatched(tokens);
        if unmatched.is_empty() {
            report_errors(&errors, tokens, file_offset);
        } else {
            unmatched.into_iter().for_each(DiagCtx::emit);
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
                ItemKind::ModuleDecl(decl) => match module {
                    None => module = Some(decl),
                    Some(_) => report_duplicate_module(item.span),
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

    /// Matches a single token of the given `kind`, yielding the token itself.
    fn kind<'a>(
        &'a self,
        k: TokenKind,
    ) -> impl ChumskyParser<'a, &'a [Token], Token, Extra<'a>> + Clone {
        any()
            .filter(move |t: &Token| t.kind == k)
            .labelled(k.describe())
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

    /// Builds a parser for `recover_with(via_parser(..))` that discards the tokens of a construct
    /// the grammar rejected and returns a placeholder node in its place.
    ///
    /// It consumes the token the grammar failed on and then every token up to the first one
    /// `stop` accepts, passing the span of all of them to `build`. Consuming that first token
    /// unconditionally is what lets the `repeated()` around a recovering parser advance.
    ///
    /// `(`, `[` and `{` groups are consumed whole, since a token `stop` accepts inside a nested
    /// group belongs to that group rather than to the construct being discarded. Stopping there
    /// would leave the group's closing delimiter behind, and the next parser would read it as
    /// the end of an enclosing construct. For the same reason a closing delimiter at depth zero
    /// always stops the skip, whatever `stop` says about it.
    fn recover_by_skipping<'a, O: 'a>(
        &'a self,
        stop: impl Fn(TokenKind) -> Option<Stop> + Clone + 'a,
        build: impl Fn(SrcSpan) -> O + Clone + 'a,
    ) -> impl ChumskyParser<'a, &'a [Token], O, Extra<'a>> + Clone + 'a {
        custom(move |inp: &mut InputRef<'a, '_, &'a [Token], Extra<'a>>| {
            // There is nothing to discard at end of input. Failing here rather than returning a
            // placeholder is what terminates the `repeated()` around a recovering parser.
            let first: Token = inp.parse(any())?;

            let mut skipped = first.span;
            let mut depth = usize::from(first.kind.opens_group());

            while let Some(token) = inp.peek() {
                if depth == 0 {
                    match stop(token.kind) {
                        Some(Stop::Before) => break,
                        Some(Stop::After) => {
                            inp.skip();
                            skipped = skipped.merge(token.span);
                            break;
                        }
                        None if token.kind.closes_group() => break,
                        None => {}
                    }
                }

                if token.kind.opens_group() {
                    depth += 1;
                } else if token.kind.closes_group() {
                    depth -= 1;
                }
                inp.skip();
                skipped = skipped.merge(token.span);
            }

            Ok(build(skipped))
        })
    }

    /// Builds the whole grammar for a single file: a sequence of items followed by end-of-input.
    ///
    /// The result is a flat list of items, including the file's `module` header and its imports.
    /// [`Self::assemble_file`] sorts those into the parts of a [`ParsedSrcFile`] afterwards.
    fn grammar<'a>(&'a self) -> impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>> + Clone {
        let item = self
            .item_parser()
            .recover_with(via_parser(self.recover_by_skipping(
                item_recovery_point,
                |span| Item {
                    id: NodeId::next(),
                    kind: ItemKind::Error,
                    span,
                },
            )));

        item.repeated().collect::<Vec<_>>().then_ignore(end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::*;
    use crate::diagnostics::{DiagCtx, Diagnostic};
    use crate::driver::source::SrcMap;
    use crate::testing::{lex_src, parse_src};

    /// Lexes and parses `src` and returns what [`DiagCtx`] collected. Unlike [`parse_src`] it
    /// asserts nothing about the result, so it can be called on source that fails to parse.
    fn diagnostics(src: &str) -> Vec<Diagnostic> {
        let (tokens, offset) = lex_src(src);
        let _ = Parser::new().parse(&tokens, offset);
        DiagCtx::diagnostics()
    }

    fn diagnostic_count(src: &str) -> usize {
        diagnostics(src).len()
    }

    /// The single diagnostic `src` raises. Panics if `src` raises any other number.
    fn only_diagnostic(src: &str) -> Diagnostic {
        let mut raised = diagnostics(src);
        assert_eq!(raised.len(), 1, "expected exactly one diagnostic");
        raised.remove(0)
    }

    /// The source text covered by a diagnostic's primary span.
    fn underlined(diagnostic: &Diagnostic) -> String {
        let span = diagnostic
            .span
            .expect("a parser diagnostic always carries a span");
        SrcMap::text_of(span).expect("the span comes from a token the lexer produced")
    }

    /// Like [`diagnostic_count`], but also returns the (best-effort, possibly error-containing)
    /// parsed unit, for exercising recovery.
    fn parse_with_errors(src: &str) -> (ParsedSrcFile, usize) {
        let (tokens, offset) = lex_src(src);
        let unit = Parser::new().parse(&tokens, offset);
        (unit, DiagCtx::diagnostics().len())
    }

    fn text(ident: Ident) -> &'static str {
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
        let segments: Vec<&str> = module.path.segments.iter().map(|s| text(*s)).collect();
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
            StmtKind::Return(Some(expr)) => {
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
            StmtKind::Return(Some(expr)) => match &expr.kind {
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
        let function_names: Vec<&str> = unit
            .items
            .iter()
            .filter_map(|item| match &item.kind {
                ItemKind::Function(f) => Some(text(f.name)),
                _ => None,
            })
            .collect();
        assert_eq!(function_names, vec!["a", "b", "c"]);
    }

    // -----------------------------------------------------------------
    // Wording
    // -----------------------------------------------------------------

    /// After a complete expression the expected set holds `;` plus every binary, postfix and
    /// assignment operator. `render_alternatives` keeps only the `;`.
    #[test]
    fn a_missing_semicolon_names_the_semicolon_and_nothing_else() {
        let diagnostic = only_diagnostic("fun main() { let x = 1 let y = 2; }");
        assert_eq!(diagnostic.message, "expected `;`, found `let`");
        assert_eq!(underlined(&diagnostic), "let");
        assert!(diagnostic.help.is_some());
    }

    /// The `labelled("an expression")` on `unary_or_new` replaces the ~20 tokens that can begin
    /// an operand, since the failure is at the operand's first token.
    #[test]
    fn a_missing_operand_asks_for_an_expression() {
        let diagnostic = only_diagnostic("fun main() { let x = 1 + ; }");
        assert_eq!(diagnostic.message, "expected an expression, found `;`");
    }

    #[test]
    fn a_missing_type_asks_for_a_type() {
        let diagnostic = only_diagnostic("fun main() { let x: = 1; }");
        assert_eq!(diagnostic.message, "expected a type, found `=`");
    }

    /// A list of nothing but keywords gets `MAX_LISTED_KEYWORDS` rather than
    /// `MAX_LISTED_ALTERNATIVES`, so all eight item keywords are named instead of four.
    #[test]
    fn a_statement_written_at_file_scope_lists_every_item_keyword() {
        let diagnostic = only_diagnostic("let x = 1;");
        assert_eq!(
            diagnostic.message,
            "expected `public`, `fun`, `struct`, `enum`, `trait`, `extend`, `module`, \
             or `import`, found `let`"
        );
    }

    #[test]
    fn a_keyword_borrowed_from_another_language_is_pointed_at_its_phi_spelling() {
        for (src, suggestion) in [("fn main() {}", "`fun`"), ("pub fun main() {}", "`public`")] {
            let help = only_diagnostic(src)
                .help
                .expect("a misspelled keyword sets `help`");
            assert!(help.contains(suggestion), "for {src:?}, got {help:?}");
        }
    }

    /// `edit_distance` charges one edit for a transposition, so `strcut` clears the threshold
    /// `is_probable_typo_of` allows for a six-character keyword.
    #[test]
    fn a_transposed_keyword_is_recognised() {
        let help = only_diagnostic("strcut P { x: i32 }")
            .help
            .expect("a misspelled keyword sets `help`");
        assert!(help.contains("`struct`"), "got {help:?}");
    }

    /// `fo` is one edit from `for`, but a `let` binding accepts an identifier, so
    /// `suggested_keyword` returns `None` and the source parses with no diagnostic at all.
    #[test]
    fn a_name_that_merely_resembles_a_keyword_is_left_alone() {
        assert_eq!(diagnostic_count("fun main() { let fo = 1; }"), 0);
    }

    #[test]
    fn a_keyword_written_where_a_name_belongs_says_so() {
        let diagnostic = only_diagnostic("fun match() {}");
        assert_eq!(diagnostic.message, "expected identifier, found `match`");
        assert_eq!(
            diagnostic.help.as_deref(),
            Some("`match` is a keyword, so it cannot be used as a name")
        );
    }

    /// The expected set here is the operators that could continue the condition plus the
    /// `labelled("a block")` on the `if` body. Only the label survives narrowing.
    #[test]
    fn a_missing_block_asks_for_the_block() {
        let diagnostic = only_diagnostic("fun main() { if x\n foo(); }");
        assert_eq!(diagnostic.message, "expected a block, found identifier");
    }

    /// Three alternatives is under `MAX_LISTED_ALTERNATIVES`, so `render_alternatives` names
    /// all of them even though `->` and `;` are terminators and `a block` is not.
    #[test]
    fn a_short_list_of_alternatives_is_enumerated() {
        let diagnostic = only_diagnostic("fun add(x: i32) i32 { return x; }");
        assert_eq!(
            diagnostic.message,
            "expected `->`, a block, or `;`, found `i32`"
        );
    }

    /// The grammar's own failure is at the end of the file; `delimiters::unmatched` replaces it
    /// with one whose span is the `{`.
    #[test]
    fn an_unclosed_brace_is_reported_at_the_brace_not_at_the_end_of_the_file() {
        let diagnostic = only_diagnostic("fun main() { return 1;");
        assert_eq!(diagnostic.message, "unclosed `{`");
        assert_eq!(underlined(&diagnostic), "{");
    }

    /// A non-empty `delimiters::unmatched` result replaces the grammar's errors rather than
    /// being emitted alongside them, so the mismatch is the only diagnostic.
    #[test]
    fn an_imbalance_suppresses_the_failures_it_causes() {
        let diagnostic = only_diagnostic("fun main() { foo(1; } fun other() {}");
        assert_eq!(
            diagnostic.message,
            "mismatched closing delimiter: expected `)`, found `}`"
        );
    }

    // -----------------------------------------------------------------
    // Recovery
    // -----------------------------------------------------------------

    /// `recover_by_skipping` tracks nesting depth, so the `}` of the nested `if` block does not
    /// stop the skip. Were it to stop there, the `}` would close the function body and every
    /// statement after it would be parsed as a top-level item.
    #[test]
    fn recovery_skips_a_nested_block_whole() {
        let (unit, error_count) =
            parse_with_errors("fun main() { let a = 1 + ; if c { g(); } let b = 2; }");
        assert_eq!(error_count, 1);

        let body = only_function(&unit)
            .block
            .as_ref()
            .expect("a `fun` item with a `{}` body has a block");
        assert!(matches!(body.stmts[0].kind, StmtKind::Error));
        assert!(matches!(body.stmts[1].kind, StmtKind::Expr { .. }));
        assert!(matches!(body.stmts[2].kind, StmtKind::Let { .. }));
    }

    /// `statement_recovery_point` returns `Stop::After` for `;`, so the skip consumes it and
    /// the following `foo();` parses as its own statement.
    #[test]
    fn recovery_stops_at_the_semicolon_that_ends_the_broken_statement() {
        let (unit, error_count) = parse_with_errors("fun main() { 1 +; foo(); }");
        assert_eq!(error_count, 1);

        let body = only_function(&unit)
            .block
            .as_ref()
            .expect("a `fun` item with a `{}` body has a block");
        assert_eq!(body.stmts.len(), 2);
        assert!(matches!(body.stmts[0].kind, StmtKind::Error));
        assert!(matches!(body.stmts[1].kind, StmtKind::Expr { .. }));
    }

    /// `recover_by_skipping` passes the merged span of every token it consumed to `build`.
    /// Later passes report against these spans, and `SrcSpan::new(0, 0)` would point them all at
    /// the first file in the `SrcMap`.
    #[test]
    fn a_recovered_statement_spans_the_source_it_replaces() {
        let (unit, _) = parse_with_errors("fun main() { 1 +; }");
        let body = only_function(&unit)
            .block
            .as_ref()
            .expect("a `fun` item with a `{}` body has a block");
        let span = body.stmts[0].span;
        assert_eq!(
            SrcMap::text_of(span).expect("the span comes from a token the lexer produced"),
            "1 +;"
        );
    }

    #[test]
    fn a_recovered_item_spans_the_source_it_replaces() {
        let (unit, _) = parse_with_errors("let x = 1; fun ok() {}");
        let span = unit.items[0].span;
        assert!(matches!(unit.items[0].kind, ItemKind::Error));
        assert_eq!(
            SrcMap::text_of(span).expect("the span comes from a token the lexer produced"),
            "let x = 1;"
        );
    }
}
