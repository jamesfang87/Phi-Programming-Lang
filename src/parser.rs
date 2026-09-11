use chumsky::Parser as ChumskyParser;
use chumsky::error::Rich;
use chumsky::extra;
use chumsky::prelude::*;

use crate::ast::{Ast, Ident, Item, ItemKind, ParsedSrcFile, Path};
use crate::diagnostics::parser::report_parse;
use crate::driver::source::SrcSpan;
use crate::lexer::describe::Descriptor;
use crate::lexer::token::{Token, TokenKind};

type Extra<'a> = extra::Err<Rich<'a, Token>>;
type BoxedP<'a, O> = Boxed<'a, 'a, &'a [Token], O, Extra<'a>>;

mod block_parser;
mod delimiters;
mod expr_parser;
mod item_parser;
mod pattern_parser;
mod recovery;
mod type_parser;

use recovery::{ITEM_RECOVERY, recover_by_skipping};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BraceForms {
    Allow,
    Deny,
}

pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Parser
    }

    #[allow(dead_code)]
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
        Ast::from(files)
    }

    fn run<'a>(
        grammar: &impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>>,
        tokens: &'a [Token],
        file_offset: usize,
    ) -> ParsedSrcFile {
        let (output, errors) = grammar.parse(tokens).into_output_errors();
        report_parse(
            &errors,
            tokens,
            file_offset,
            delimiters::find_unmatched_delimiter_errors(tokens),
        );

        match output {
            Some(items) => ParsedSrcFile::from_items(items, file_offset),
            None => ParsedSrcFile {
                module: None,
                imports: Vec::new(),
                items: Vec::new(),
                span: SrcSpan::new(file_offset, file_offset),
            },
        }
    }

    /// Matches a single token of the given `kind`, yielding the token itself.
    fn kind<'a>(
        &'a self,
        k: TokenKind,
    ) -> impl ChumskyParser<'a, &'a [Token], Token, Extra<'a>> + Clone {
        any()
            .filter(move |t: &Token| t.kind == k)
            .labelled(Descriptor::of(k).describe())
    }

    fn ident_parser<'a>(&'a self) -> BoxedP<'a, Ident> {
        self.kind(TokenKind::Identifier)
            .map(Ident::of_token)
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

    fn grammar<'a>(&'a self) -> impl ChumskyParser<'a, &'a [Token], Vec<Item>, Extra<'a>> + Clone {
        let item = self.item_parser().recover_with(via_parser(
            recover_by_skipping(ITEM_RECOVERY, |span| {
                Item::new(ItemKind::Error, span)
            }),
        ));

        item.repeated().collect::<Vec<_>>().then_ignore(end())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::interner::Interner;
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

    /// A name run directly into a number (`1e5` reads as `1` glued to a name) is a missing
    /// operator or `;`, or an attempt at scientific notation, which Phi does not have. The
    /// help must say that rather than suggest a missing `;` between unrelated statements.
    #[test]
    fn a_name_glued_to_a_number_says_what_is_missing() {
        let diagnostic = only_diagnostic("fun main() { let x = 1e5; }");
        assert_eq!(diagnostic.message, "expected `;`, found identifier");
        assert_eq!(
            diagnostic.help.as_deref(),
            Some(
                "a number cannot be followed directly by a name; an operator or `;` is missing here"
            )
        );
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

    /// `STATEMENT_RECOVERY` consumes the `;` that ends the broken statement, so the following
    /// `foo();` parses as its own statement.
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
