use crate::ast::Block;

use super::{BoxedP, Parser};

impl Parser {
    pub fn block_parser<'a>(&'a self) -> BoxedP<'a, Block> {
        self.expr_and_block_parsers().1
    }
}

#[cfg(test)]
mod tests {
    use chumsky::Parser as ChumskyParser;

    use super::*;
    use crate::ast::{BinaryOp, DeclStmt, ExprKind, Literal, Mutability, PatternKind, Stmt, StmtKind};
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::interner::Interner;
    use crate::lexer::Lexer;

    fn parse_block(src: &str) -> Block {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
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

    /// Like [`parse_block`], but doesn't assert the parse was clean — for exercising recovery.
    fn parse_block_with_errors(src: &str) -> (Block, usize) {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
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
    fn parses_for_stmt_with_binding_pattern() {
        let block = parse_block("{ for x in xs { foo(x); } }");
        match &only_stmt(&block).kind {
            StmtKind::For { name, iter, body } => {
                assert!(matches!(name.kind, PatternKind::Binding(_)));
                assert!(matches!(iter.kind, ExprKind::DeclRef(_)));
                assert_eq!(body.stmts.len(), 1);
            }
            other => panic!("expected a for statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_for_stmt_with_tuple_pattern() {
        let block = parse_block("{ for (a, b) in pairs { foo(); } }");
        match &only_stmt(&block).kind {
            StmtKind::For { name, .. } => {
                assert!(matches!(name.kind, PatternKind::Tuple(_)));
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
            StmtKind::Return { ret } => {
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
            StmtKind::Defer { defer } => {
                assert!(matches!(defer.kind, ExprKind::FunCall { .. }));
            }
            other => panic!("expected a defer statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_immutable_decl_stmt() {
        let block = parse_block("{ let x = 1; }");
        match &only_stmt(&block).kind {
            StmtKind::Decl(DeclStmt {
                mutability,
                name,
                ty,
                ..
            }) => {
                assert!(matches!(mutability, Mutability::Immutable));
                assert!(matches!(name.kind, PatternKind::Binding(_)));
                assert!(ty.is_none());
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_decl_stmt_with_type_annotation() {
        let block = parse_block("{ let mut x: i32 = 1; }");
        match &only_stmt(&block).kind {
            StmtKind::Decl(DeclStmt {
                mutability, ty, ..
            }) => {
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
            StmtKind::Decl(DeclStmt { name, .. }) => {
                assert!(matches!(name.kind, PatternKind::Tuple(_)));
            }
            other => panic!("expected a let statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_with_stmt_with_single_lend() {
        let block = parse_block("{ with x = &y { foo(x); } }");
        match &only_stmt(&block).kind {
            StmtKind::With { lends, body } => {
                assert_eq!(lends.len(), 1);
                assert!(matches!(lends[0].name.kind, PatternKind::Binding(_)));
                assert!(matches!(lends[0].expr.kind, ExprKind::Borrow { .. }));
                assert_eq!(body.stmts.len(), 1);
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
                match &lends[0].expr.kind {
                    ExprKind::Borrow { mutability, .. } => {
                        assert!(matches!(mutability, Mutability::Immutable))
                    }
                    other => panic!("expected a borrow expr, got {other:?}"),
                }
                match &lends[1].expr.kind {
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
            StmtKind::Expr(expr) => {
                assert!(matches!(expr.kind, ExprKind::FunCall { .. }));
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
            StmtKind::While { body, .. } => {
                assert_eq!(body.stmts.len(), 2);
                assert!(matches!(body.stmts[0].kind, StmtKind::Decl(_)));
                assert!(matches!(body.stmts[1].kind, StmtKind::While { .. }));
            }
            other => panic!("expected a while statement, got {other:?}"),
        }
    }

    #[test]
    fn parses_multiple_statements_in_order() {
        let block = parse_block("{ let x = 1; let y = 2; return x + y; }");
        assert_eq!(block.stmts.len(), 3);
        assert!(matches!(block.stmts[0].kind, StmtKind::Decl(_)));
        assert!(matches!(block.stmts[1].kind, StmtKind::Decl(_)));
        assert!(matches!(block.stmts[2].kind, StmtKind::Return { .. }));
    }

    #[test]
    fn parses_decl_with_literal_pattern_expr_value() {
        let block = parse_block(r#"{ let s = "hi\n"; }"#);
        match &only_stmt(&block).kind {
            StmtKind::Decl(DeclStmt { expr, .. }) => match &expr.kind {
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
        assert!(matches!(block.stmts[0].kind, StmtKind::Decl(_)));
        assert!(matches!(block.stmts[1].kind, StmtKind::Error));
        assert!(matches!(block.stmts[2].kind, StmtKind::Return { .. }));
    }

    #[test]
    fn recovers_from_a_missing_semicolon_and_keeps_parsing_later_statements() {
        let (block, error_count) = parse_block_with_errors("{ let x = 1 let y = 2; }");
        assert_eq!(error_count, 1);
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(block.stmts[0].kind, StmtKind::Error));
        assert!(matches!(block.stmts[1].kind, StmtKind::Decl(_)));
    }

    #[test]
    fn recovery_does_not_disturb_a_trailing_tail_expression() {
        // A semicolon-less tail expression must still come through as the block's final
        // statement, not get swallowed by statement-recovery.
        let block = parse_block("{ let x = 1; x }");
        assert_eq!(block.stmts.len(), 2);
        assert!(matches!(block.stmts[0].kind, StmtKind::Decl(_)));
        assert!(matches!(block.stmts[1].kind, StmtKind::Expr(_)));
    }
}
