//! Exposes the block parser on its own.
//!
//! The real block grammar lives in `expr_parser`, next to the expression grammar it recurses
//! with (a block holds statements, and statements hold expressions, and expressions can hold
//! blocks). This file just gives that parser its own name and its own tests.

use crate::ast::Block;

use super::{BoxedP, Parser};

impl Parser {
    /// Parses a `{ ... }` block: a sequence of statements plus an optional tail expression.
    pub fn block_parser<'a>(&'a self) -> BoxedP<'a, Block> {
        self.expr_and_block_parsers().1
    }
}

#[cfg(test)]
mod tests {
    use chumsky::Parser as ChumskyParser;

    use super::*;
    use crate::ast::interner::Interner;
    use crate::ast::{BinaryOp, Expr, ExprKind, Literal, Mutability, PatKind, Stmt, StmtKind};
    use crate::testing::lex_src;

    fn parse_block(src: &str) -> Block {
        let (tokens, _) = lex_src(src);
        let parser = Parser::new();
        let (output, errors) = parser
            .block_parser()
            .parse(&tokens[..])
            .into_output_errors();
        assert!(
            errors.is_empty(),
            "unexpected parse errors for {src:?}: {errors:?}"
        );
        output.expect("expected a successfully parsed block")
    }

    /// Like [`parse_block`], but doesn't assert the parse was clean, for exercising recovery.
    fn parse_block_with_errors(src: &str) -> (Block, usize) {
        let (tokens, _) = lex_src(src);
        let parser = Parser::new();
        let (output, errors) = parser
            .block_parser()
            .parse(&tokens[..])
            .into_output_errors();
        (
            output.expect("expected recovery to still produce a block"),
            errors.len(),
        )
    }

    fn only_stmt(block: &Block) -> &Stmt {
        assert_eq!(block.stmts.len(), 1);
        &block.stmts[0]
    }

    #[test]
    fn parses_while_stmt() {
        let block = parse_block("{ while x < 5 { foo(); } }");
        match &only_stmt(&block).kind {
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

    /// A `}` already ends these unambiguously, so no `;` is needed to make one a statement.
    #[test]
    fn block_bodied_expressions_are_statements_without_a_semicolon() {
        for src in [
            "{ if c { g(); } g(); }",
            "{ if let .some(x) = o { g(); } g(); }",
            "{ match c { _ => 1 } g(); }",
            "{ { g(); } g(); }",
            "{ concurrent { g(); } g(); }",
            "{ spawn { g(); } g(); }",
        ] {
            let block = parse_block(src);
            assert_eq!(block.stmts.len(), 2, "for {src:?}");
            assert!(
                matches!(block.stmts[0].kind, StmtKind::Expr { .. }),
                "for {src:?}"
            );
        }
    }

    /// Everything else still needs one. A `;`-less trailing expression is the block's value,
    /// not a statement, so it must stay the last thing in the block.
    #[test]
    fn other_expressions_still_need_a_semicolon_to_be_statements() {
        let block = parse_block("{ g(); h() }");
        assert_eq!(block.stmts.len(), 2);

        let (_, errors) = parse_block_with_errors("{ g() h(); }");
        assert!(errors > 0, "a bare call statement should still want a `;`");
    }

    /// A block-bodied statement doesn't consume the block's value.
    #[test]
    fn block_bodied_statement_leaves_the_tail_expression_alone() {
        let block = parse_block("{ if c { 1 } else { 2 } 5 }");
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(
            block.stmts[0].kind,
            StmtKind::Expr {
                expr: Expr {
                    kind: ExprKind::If { .. },
                    ..
                },
                semi: false,
            }
        ));
        assert!(matches!(
            block.stmts[1].kind,
            StmtKind::Expr {
                expr: Expr {
                    kind: ExprKind::Literal(_),
                    ..
                },
                semi: false,
            }
        ));
    }

    #[test]
    fn parses_while_let_stmt() {
        let block = parse_block("{ while let .some(x) = next() { foo(x); } }");
        match &only_stmt(&block).kind {
            StmtKind::WhileLet {
                pat,
                scrutinee,
                block,
            } => {
                assert!(matches!(pat.kind, PatKind::Variant { .. }));
                assert!(matches!(scrutinee.kind, ExprKind::Call { .. }));
                assert_eq!(block.stmts.len(), 1);
            }
            other => panic!("expected a while-let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_for_stmt_with_binding_pattern() {
        let block = parse_block("{ for x in xs { foo(x); } }");
        match &only_stmt(&block).kind {
            StmtKind::For { pat, iter, block } => {
                assert!(matches!(pat.kind, PatKind::Binding(_)));
                assert!(matches!(iter.kind, ExprKind::Path(_)));
                assert_eq!(block.stmts.len(), 1);
            }
            other => panic!("expected a for statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_for_stmt_with_tuple_pattern() {
        let block = parse_block("{ for (a, b) in pairs { foo(); } }");
        match &only_stmt(&block).kind {
            StmtKind::For { pat, .. } => {
                assert!(matches!(pat.kind, PatKind::Tuple(_)));
            }
            other => panic!("expected a for statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_break_stmt() {
        let block = parse_block("{ break; }");
        assert!(matches!(only_stmt(&block).kind, StmtKind::Break));
    }

    #[test]
    fn parses_continue_stmt() {
        let block = parse_block("{ continue; }");
        assert!(matches!(only_stmt(&block).kind, StmtKind::Continue));
    }

    #[test]
    fn parses_return_stmt() {
        let block = parse_block("{ return 1 + 2; }");
        match &only_stmt(&block).kind {
            StmtKind::Return(ret) => {
                assert!(matches!(
                    ret.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            other => panic!("expected a return statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_defer_stmt() {
        let block = parse_block("{ defer cleanup(); }");
        match &only_stmt(&block).kind {
            StmtKind::Defer(defer) => {
                assert!(matches!(defer.kind, ExprKind::Call { .. }));
            }
            other => panic!("expected a defer statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_immutable_decl_stmt() {
        let block = parse_block("{ let x = 1; }");
        match &only_stmt(&block).kind {
            StmtKind::Let {
                mutability,
                pat,
                ty,
                ..
            } => {
                assert!(matches!(mutability, Mutability::Immutable));
                assert!(matches!(pat.kind, PatKind::Binding(_)));
                assert!(ty.is_none());
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_decl_stmt_with_type_annotation() {
        let block = parse_block("{ let mut x: i32 = 1; }");
        match &only_stmt(&block).kind {
            StmtKind::Let { mutability, ty, .. } => {
                assert!(matches!(mutability, Mutability::Mutable));
                assert!(ty.is_some());
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_decl_stmt_with_tuple_pattern() {
        let block = parse_block("{ let (x, y) = point; }");
        match &only_stmt(&block).kind {
            StmtKind::Let { pat, .. } => {
                assert!(matches!(pat.kind, PatKind::Tuple(_)));
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_with_stmt_with_single_lend() {
        let block = parse_block("{ with x = &y { foo(x); } }");
        match &only_stmt(&block).kind {
            StmtKind::With { lends, block } => {
                assert_eq!(lends.len(), 1);
                assert!(matches!(lends[0].pat.kind, PatKind::Binding(_)));
                assert!(matches!(lends[0].init.kind, ExprKind::Borrow { .. }));
                assert_eq!(block.stmts.len(), 1);
            }
            other => panic!("expected a with statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_with_stmt_with_multiple_lends() {
        let block = parse_block("{ with x = &a, y = &mut b { foo(); } }");
        match &only_stmt(&block).kind {
            StmtKind::With { lends, .. } => {
                assert_eq!(lends.len(), 2);
                match &lends[0].init.kind {
                    ExprKind::Borrow { mutability, .. } => {
                        assert!(matches!(mutability, Mutability::Immutable))
                    }
                    other => panic!("expected a borrow expr, got {other:?}"),
                }
                match &lends[1].init.kind {
                    ExprKind::Borrow { mutability, .. } => {
                        assert!(matches!(mutability, Mutability::Mutable))
                    }
                    other => panic!("expected a borrow expr, got {other:?}"),
                }
            }
            other => panic!("expected a with statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_expr_stmt() {
        let block = parse_block(r#"{ println("hi"); }"#);
        match &only_stmt(&block).kind {
            StmtKind::Expr { expr, .. } => {
                assert!(matches!(expr.kind, ExprKind::Call { .. }));
            }
            other => panic!("expected an expr statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_block_inside_while_body() {
        // Exercises the block parser's recursion: a `while` whose body contains a `let` and a
        // nested `while`.
        let block = parse_block("{ while true { let x = 1; while x < 2 { x; } } }");
        match &only_stmt(&block).kind {
            StmtKind::While { block, .. } => {
                assert_eq!(block.stmts.len(), 2);
                assert!(matches!(block.stmts[0].kind, StmtKind::Let { .. }));
                assert!(matches!(block.stmts[1].kind, StmtKind::While { .. }));
            }
            other => panic!("expected a while statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_statements_in_order() {
        let block = parse_block("{ let x = 1; let y = 2; return x + y; }");
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(block.stmts[0].kind, StmtKind::Let { .. }));
        assert!(matches!(block.stmts[1].kind, StmtKind::Let { .. }));
        assert!(matches!(block.stmts[2].kind, StmtKind::Return(..)));
    }

    #[test]
    fn parses_decl_with_literal_pattern_expr_value() {
        let block = parse_block(r#"{ let s = "hi\n"; }"#);
        match &only_stmt(&block).kind {
            StmtKind::Let { init, .. } => match &init.kind {
                ExprKind::Literal(Literal::Str(sym)) => {
                    assert_eq!(Interner::resolve(*sym), "hi\n")
                }
                other => panic!("expected a string literal, got {other:?}"),
            },
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn recovers_from_a_malformed_statement_and_keeps_parsing_later_ones() {
        // `1 +;` is broken (a dangling `+` with no right-hand side before the `;`); the `let`
        // and `return` statements on either side of it should still show up in the tree.
        let (block, error_count) = parse_block_with_errors("{ let a = 1; 1 +; return a; }");
        assert_eq!(error_count, 1);
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(block.stmts[0].kind, StmtKind::Let { .. }));
        assert!(matches!(block.stmts[1].kind, StmtKind::Error));
        assert!(matches!(block.stmts[2].kind, StmtKind::Return(..)));
    }

    #[test]
    fn recovers_from_a_missing_semicolon_and_keeps_parsing_later_statements() {
        let (block, error_count) = parse_block_with_errors("{ let x = 1 let y = 2; }");
        assert_eq!(error_count, 1);
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(block.stmts[0].kind, StmtKind::Error));
        assert!(matches!(block.stmts[1].kind, StmtKind::Let { .. }));
    }

    #[test]
    fn recovery_does_not_disturb_a_trailing_tail_expression() {
        // A semicolon-less tail expression must still come through as the block's final
        // statement, not get swallowed by statement-recovery.
        let block = parse_block("{ let x = 1; x }");
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(block.stmts[0].kind, StmtKind::Let { .. }));
        assert!(matches!(block.stmts[1].kind, StmtKind::Expr { .. }));
    }
}
