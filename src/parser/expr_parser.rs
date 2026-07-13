use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;

use crate::ast::{
    BinaryOp, Block, DeclStmt, Expr, ExprKind, Function, Ident, Item, ItemKind, Literal,
    Mutability, Param, Path, Pattern, PatternKind, SrcUnit, Stmt, StmtKind, Ty, Type, UnaryOp,
    Visibility,
};

use crate::lexer::token::{Token, TokenKind};
use crate::parser::combine_binary;

use super::{BoxedP, Extra, Parser};

impl Parser {
    pub fn expr_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        let path = self.path_parser();

        recursive(
            |expr: Recursive<dyn ChumskyParser<'a, &'a [Token], Expr, Extra<'a>>>| {
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
                .boxed();

                let call = path
                    .clone()
                    .then_ignore(self.kind(TokenKind::OpenParen))
                    .then(
                        expr.clone()
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((callee_path, args), close_tok)| {
                        let callee_span = callee_path.span;
                        let span = callee_span.merge(close_tok.span);
                        Expr {
                            kind: ExprKind::FunCall {
                                callee: Box::new(Expr {
                                    kind: ExprKind::DeclRef(callee_path),
                                    span: callee_span,
                                }),
                                args,
                            },
                            span,
                        }
                    })
                    .boxed();

                let decl_ref = path.clone().map(|p: Path| {
                    let span = p.span;
                    Expr {
                        kind: ExprKind::DeclRef(p),
                        span,
                    }
                });

                let group = self
                    .kind(TokenKind::OpenParen)
                    .then(expr.clone())
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|((_open, e), _close)| e)
                    .boxed();

                let atom = choice((literal, call, decl_ref, group)).boxed();

                let unary_op = choice((
                    self.kind(TokenKind::Minus)
                        .map(|t: Token| (UnaryOp::Neg, t.span)),
                    self.kind(TokenKind::Bang)
                        .map(|t: Token| (UnaryOp::Not, t.span)),
                ));
                let unary = unary_op
                    .repeated()
                    .foldr(atom, |(op, op_span), operand| {
                        let span = op_span.merge(operand.span);
                        Expr {
                            kind: ExprKind::Unary {
                                op,
                                operand: Box::new(operand),
                            },
                            span,
                        }
                    })
                    .boxed();

                let bin_op =
                    |k: TokenKind, op: BinaryOp| self.kind(k).map(move |t: Token| (op, t.span));

                let mul_op = choice((
                    bin_op(TokenKind::Star, BinaryOp::Mul),
                    bin_op(TokenKind::Slash, BinaryOp::Div),
                    bin_op(TokenKind::Percent, BinaryOp::Rem),
                ));
                let product = unary
                    .clone()
                    .foldl(mul_op.then(unary.clone()).repeated(), combine_binary)
                    .boxed();

                let add_op = choice((
                    bin_op(TokenKind::Plus, BinaryOp::Add),
                    bin_op(TokenKind::Minus, BinaryOp::Sub),
                ));
                let sum = product
                    .clone()
                    .foldl(add_op.then(product.clone()).repeated(), combine_binary)
                    .boxed();

                let cmp_op = choice((
                    bin_op(TokenKind::DoubleEquals, BinaryOp::Eq),
                    bin_op(TokenKind::BangEquals, BinaryOp::Ne),
                    bin_op(TokenKind::LessEqual, BinaryOp::Le),
                    bin_op(TokenKind::GreaterEqual, BinaryOp::Ge),
                    bin_op(TokenKind::OpenCaret, BinaryOp::Lt),
                    bin_op(TokenKind::CloseCaret, BinaryOp::Gt),
                ));
                let comparison = sum
                    .clone()
                    .foldl(cmp_op.then(sum.clone()).repeated(), combine_binary)
                    .boxed();

                let and_op = bin_op(TokenKind::DoubleAmp, BinaryOp::And);
                let logical_and = comparison
                    .clone()
                    .foldl(and_op.then(comparison.clone()).repeated(), combine_binary)
                    .boxed();

                let or_op = bin_op(TokenKind::DoublePipe, BinaryOp::Or);
                logical_and
                    .clone()
                    .foldl(or_op.then(logical_and.clone()).repeated(), combine_binary)
            },
        )
        .boxed()
    }
}
