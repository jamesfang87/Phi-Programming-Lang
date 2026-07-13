use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{Block, DeclStmt, Mutability, Pattern, PatternKind, Stmt, StmtKind};

use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

impl Parser {
    pub fn block_parser<'a>(&'a self) -> BoxedP<'a, Block> {
        let ident = self.ident_parser();
        let expr = self.expr_parser();
        let type_p = self.type_parser();

        recursive(
            |block: Recursive<dyn ChumskyParser<'a, &'a [Token], Block, Extra<'a>>>| {
                let return_stmt = self
                    .kind(TokenKind::ReturnKw)
                    .then(expr.clone())
                    .then(self.kind(TokenKind::Semicolon))
                    .map(|((ret_tok, value), semi_tok)| {
                        let span = ret_tok.span.merge(semi_tok.span);
                        Stmt {
                            kind: StmtKind::Return(value),
                            span,
                        }
                    })
                    .boxed();

                let decl_stmt = self
                    .kind(TokenKind::LetKw)
                    .then(self.kind(TokenKind::MutKw).or_not())
                    .then(ident.clone())
                    .then(
                        self.kind(TokenKind::Colon)
                            .ignore_then(type_p.clone())
                            .or_not(),
                    )
                    .then_ignore(self.kind(TokenKind::Equals))
                    .then(expr.clone())
                    .then(self.kind(TokenKind::Semicolon))
                    .map(|(((((let_tok, mut_tok), name), ty), value), semi_tok)| {
                        let mutability = if mut_tok.is_some() {
                            Mutability::Mutable
                        } else {
                            Mutability::Immutable
                        };
                        let span = let_tok.span.merge(semi_tok.span);
                        let decl = DeclStmt {
                            mutability,
                            name: Pattern {
                                kind: PatternKind::Binding(name),
                                span: name.span,
                            },
                            ty,
                            expr: value,
                            span,
                        };
                        Stmt {
                            kind: StmtKind::Decl(decl),
                            span,
                        }
                    })
                    .boxed();

                let while_stmt = self
                    .kind(TokenKind::WhileKw)
                    .then(expr.clone())
                    .then(block.clone())
                    .map(|((while_tok, cond), body)| {
                        let span = while_tok.span.merge(body.span);
                        Stmt {
                            kind: StmtKind::While { cond, body },
                            span,
                        }
                    })
                    .boxed();

                let expr_stmt = expr
                    .clone()
                    .then(self.kind(TokenKind::Semicolon))
                    .map(|(value, semi_tok)| {
                        let span = value.span.merge(semi_tok.span);
                        Stmt {
                            kind: StmtKind::Expr(value),
                            span,
                        }
                    })
                    .boxed();

                let stmt = choice((return_stmt, decl_stmt, while_stmt, expr_stmt)).boxed();

                self.kind(TokenKind::OpenBrace)
                    .then(stmt.repeated().collect::<Vec<_>>())
                    .then(self.kind(TokenKind::CloseBrace))
                    .map(|((open_tok, stmts), close_tok)| Block {
                        stmts,
                        span: open_tok.span.merge(close_tok.span),
                    })
            },
        )
        .boxed()
    }
}
