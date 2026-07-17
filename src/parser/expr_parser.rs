use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;
use chumsky::recursive::Indirect;

use crate::ast::{
    BinaryOp, Block, ClosureParam, CtorPayload, DeclStmt, Expr, ExprKind, Ident, Literal, MatchArm,
    Mutability, Path, Stmt, StmtKind, UnaryOp, WithStmtLend,
};

use crate::ast::interner::Interner;
use crate::driver::src_map::SrcMap;
use crate::lexer::src_span::SrcSpan;
use crate::lexer::token::{Token, TokenKind};
use crate::parser::combine_binary;

use super::{BoxedP, Extra, Parser};

type ExprRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Expr, Extra<'a>>>;
type BlockRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Block, Extra<'a>>>;

impl Parser {
    pub fn expr_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        self.expr_and_block_parsers().0
    }

    pub(crate) fn expr_and_block_parsers<'a>(&'a self) -> (BoxedP<'a, Expr>, BoxedP<'a, Block>) {
        let mut expr: ExprRec<'a> = Recursive::declare();
        let mut block: BlockRec<'a> = Recursive::declare();

        let path = self.path_parser();
        let ident = self.ident_parser();
        let pattern = self.pattern_parser();

        let expr_body = {
            let expr = expr.clone();
            let block = block.clone();
            let type_p = self.type_parser_with_expr(expr.clone().boxed());

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

            // `self` used as a value, e.g. `self.x` inside a method body.
            let self_expr = self
                .kind(TokenKind::LowerSelfKw)
                .map(|t: Token| {
                    let name = Ident {
                        text: Interner::intern(
                            &SrcMap::text_of(t.span)
                                .expect("lexer token span should always resolve to a source file"),
                        ),
                        span: t.span,
                    };
                    Expr {
                        kind: ExprKind::DeclRef(Path {
                            segments: vec![name],
                            span: t.span,
                        }),
                        span: t.span,
                    }
                })
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

            // `Path { field: expr, ... }` — struct/enum-variant construction.
            //
            // NOTE: like Rust, this makes a bare `Path { ... }` ambiguous with the body of an
            // `if`/`while`/`match` whose condition is that same bare path (`if Foo { ... }`
            // could be read as the ctor `Foo { ... }` used as the condition, or as a
            // condition `Foo` followed by an empty `if` body). We don't special-case
            // condition position the way Rust's parser does, so callers should parenthesize
            // a ctor expression used directly as a condition/scrutinee to avoid this.
            let ctor_field = ident
                .clone()
                .then_ignore(self.kind(TokenKind::Colon))
                .then(expr.clone())
                .map(|(name, value)| {
                    let span = name.span.merge(value.span);
                    CtorPayload {
                        name,
                        expr: Box::new(value),
                        span,
                    }
                })
                .boxed();

            let ctor = path
                .clone()
                .then_ignore(self.kind(TokenKind::OpenBrace))
                .then(
                    ctor_field
                        .separated_by(self.kind(TokenKind::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then(self.kind(TokenKind::CloseBrace))
                .map(|((ctor_path, payload), close_tok)| {
                    let span = ctor_path.span.merge(close_tok.span);
                    Expr {
                        kind: ExprKind::Ctor {
                            path: ctor_path,
                            payload,
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

            // `(expr)` groups; `(expr, expr, ...)` (zero or ≥2 elements) is a tuple.
            let tuple_or_group = self
                .kind(TokenKind::OpenParen)
                .then(
                    expr.clone()
                        .separated_by(self.kind(TokenKind::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then(self.kind(TokenKind::CloseParen))
                .map(|((open_tok, mut exprs), close_tok)| {
                    if exprs.len() == 1 {
                        exprs.pop().expect("checked len == 1 above")
                    } else {
                        Expr {
                            kind: ExprKind::Tuple(exprs),
                            span: open_tok.span.merge(close_tok.span),
                        }
                    }
                })
                .boxed();

            // A bare `{ ... }` used directly as an expression.
            let block_expr = block
                .clone()
                .map(|b: Block| {
                    let span = b.span;
                    Expr {
                        kind: ExprKind::Block(b),
                        span,
                    }
                })
                .boxed();

            let if_expr = self
                .kind(TokenKind::IfKw)
                .then(expr.clone())
                .then(block.clone())
                .then(
                    self.kind(TokenKind::ElseKw)
                        .ignore_then(choice((
                            block.clone().map(|b: Block| {
                                let span = b.span;
                                Expr {
                                    kind: ExprKind::Block(b),
                                    span,
                                }
                            }),
                            expr.clone(),
                        )))
                        .or_not(),
                )
                .map(|(((if_tok, cond), then_branch), else_branch)| {
                    let span = match &else_branch {
                        Some(e) => if_tok.span.merge(e.span),
                        None => if_tok.span.merge(then_branch.span),
                    };
                    Expr {
                        kind: ExprKind::If {
                            cond: Box::new(cond),
                            then_branch,
                            else_branch: else_branch.map(Box::new),
                        },
                        span,
                    }
                })
                .boxed();

            let match_arm = pattern
                .clone()
                .then_ignore(self.kind(TokenKind::FatArrow))
                .then(expr.clone())
                .map(|(pat, body)| {
                    let span = pat.span.merge(body.span);
                    MatchArm {
                        pat,
                        body: Box::new(body),
                        span,
                    }
                })
                .boxed();

            let match_expr = self
                .kind(TokenKind::MatchKw)
                .then(expr.clone())
                .then_ignore(self.kind(TokenKind::OpenBrace))
                .then(
                    match_arm
                        .separated_by(self.kind(TokenKind::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then(self.kind(TokenKind::CloseBrace))
                .map(|(((match_tok, scrutinee), arms), close_tok)| {
                    let span = match_tok.span.merge(close_tok.span);
                    Expr {
                        kind: ExprKind::Match {
                            scrutinee: Box::new(scrutinee),
                            arms,
                        },
                        span,
                    }
                })
                .boxed();

            let spawn_expr = self
                .kind(TokenKind::SpawnKw)
                .then(block.clone())
                .map(|(spawn_tok, body)| {
                    let span = spawn_tok.span.merge(body.span);
                    Expr {
                        kind: ExprKind::Spawn(body),
                        span,
                    }
                })
                .boxed();

            let concurrent_expr = self
                .kind(TokenKind::ConcurrentKw)
                .then(block.clone())
                .map(|(concurrent_tok, body)| {
                    let span = concurrent_tok.span.merge(body.span);
                    Expr {
                        kind: ExprKind::Concurrent(body),
                        span,
                    }
                })
                .boxed();

            // Rust-like closures: `|x: i32, y: i32| -> i32 { x + y }`, `|x| x + 1`, `|| 42`.
            // Parameter types and the return type are optional (inferred). The body is any
            // expr, so a `{ ... }` block body falls out of `block_expr` for free.
            let closure_param = ident
                .clone()
                .then(
                    self.kind(TokenKind::Colon)
                        .ignore_then(type_p.clone())
                        .or_not(),
                )
                .map(|(name, ty)| {
                    let span = match &ty {
                        Some(ty) => name.span.merge(ty.span),
                        None => name.span,
                    };
                    ClosureParam { name, ty, span }
                });

            // `||` lexes as a single `DoublePipe` token, so an empty parameter list can't be
            // spelled as two `Pipe` tokens — it has to be special-cased, exactly like Rust's
            // own parser does for the same reason.
            let closure_params = choice((
                self.kind(TokenKind::DoublePipe)
                    .map(|t: Token| (Vec::new(), t.span)),
                self.kind(TokenKind::Pipe)
                    .then(
                        closure_param
                            .separated_by(self.kind(TokenKind::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then(self.kind(TokenKind::Pipe))
                    .map(|((open_tok, params), close_tok)| {
                        (params, open_tok.span.merge(close_tok.span))
                    }),
            ));

            let closure = closure_params
                .then(
                    self.kind(TokenKind::Arrow)
                        .ignore_then(type_p.clone())
                        .or_not(),
                )
                .then(expr.clone())
                .map(|(((params, params_span), ret), body)| {
                    let span = params_span.merge(body.span);
                    Expr {
                        kind: ExprKind::Closure {
                            params,
                            ret,
                            body: Box::new(body),
                        },
                        span,
                    }
                })
                .boxed();

            let atom = choice((
                closure,
                literal,
                if_expr,
                match_expr,
                spawn_expr,
                concurrent_expr,
                call,
                ctor,
                self_expr,
                decl_ref,
                tuple_or_group,
                block_expr,
            ))
            .boxed();

            // Postfix operators: `.field`, `.method(args)`, `[index]`, `?`. These bind
            // tighter than any prefix operator, so `-x.y` is `-(x.y)`.
            enum Postfix {
                Field(Ident),
                Method(Ident, Vec<Expr>),
                Index(Expr),
                Try,
            }

            let method_or_field = self
                .kind(TokenKind::Period)
                .ignore_then(ident.clone())
                .then(
                    self.kind(TokenKind::OpenParen)
                        .ignore_then(
                            expr.clone()
                                .separated_by(self.kind(TokenKind::Comma))
                                .allow_trailing()
                                .collect::<Vec<_>>(),
                        )
                        .then(self.kind(TokenKind::CloseParen))
                        .or_not(),
                )
                .map(|(name, call_part)| match call_part {
                    Some((args, close_tok)) => (Postfix::Method(name, args), close_tok.span),
                    None => {
                        let span = name.span;
                        (Postfix::Field(name), span)
                    }
                });

            let index_op = self
                .kind(TokenKind::OpenBracket)
                .ignore_then(expr.clone())
                .then(self.kind(TokenKind::CloseBracket))
                .map(|(index, close_tok)| (Postfix::Index(index), close_tok.span));

            let try_op = self
                .kind(TokenKind::Try)
                .map(|t: Token| (Postfix::Try, t.span));

            let postfix_op = choice((method_or_field, index_op, try_op));

            let postfix = atom
                .foldl(postfix_op.repeated(), |receiver, (op, op_span)| {
                    let span = receiver.span.merge(op_span);
                    match op {
                        Postfix::Field(field) => Expr {
                            kind: ExprKind::Field {
                                base: Box::new(receiver),
                                field,
                            },
                            span,
                        },
                        Postfix::Method(method, args) => Expr {
                            kind: ExprKind::MethodCall {
                                receiver: Box::new(receiver),
                                method,
                                args,
                            },
                            span,
                        },
                        Postfix::Index(index) => Expr {
                            kind: ExprKind::Index {
                                base: Box::new(receiver),
                                index: Box::new(index),
                            },
                            span,
                        },
                        Postfix::Try => Expr {
                            kind: ExprKind::Try(Box::new(receiver)),
                            span,
                        },
                    }
                })
                .boxed();

            // Prefix operators: `-`, `!`, `&`, `&mut`. `&`/`&mut` build a `Borrow` rather
            // than a `Unary`, so we fold over a small local enum instead of `UnaryOp` alone.
            enum Prefix {
                Unary(UnaryOp),
                Borrow(Mutability),
            }

            let prefix_op = choice((
                self.kind(TokenKind::Minus)
                    .map(|t: Token| (Prefix::Unary(UnaryOp::Neg), t.span)),
                self.kind(TokenKind::Bang)
                    .map(|t: Token| (Prefix::Unary(UnaryOp::Not), t.span)),
                self.kind(TokenKind::Amp)
                    .then(self.kind(TokenKind::MutKw).or_not())
                    .map(|(amp_tok, mut_tok)| {
                        let mutability = if mut_tok.is_some() {
                            Mutability::Mutable
                        } else {
                            Mutability::Immutable
                        };
                        (Prefix::Borrow(mutability), amp_tok.span)
                    }),
            ));
            let unary = prefix_op
                .repeated()
                .foldr(postfix, |(op, op_span), operand| {
                    let span = op_span.merge(operand.span);
                    match op {
                        Prefix::Unary(op) => Expr {
                            kind: ExprKind::Unary {
                                op,
                                operand: Box::new(operand),
                            },
                            span,
                        },
                        Prefix::Borrow(mutability) => Expr {
                            kind: ExprKind::Borrow {
                                mutability,
                                operand: Box::new(operand),
                            },
                            span,
                        },
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
            let logical_or = logical_and
                .clone()
                .foldl(or_op.then(logical_and.clone()).repeated(), combine_binary)
                .boxed();

            // Ranges: `a..b`, `a..=b`, `a..`, `..b`, `..=b`, `..`. Lowest precedence, and
            // non-associative — `a..b..c` isn't meaningful and isn't accepted.
            let range_op = choice((
                self.kind(TokenKind::InclRange)
                    .map(|t: Token| (true, t.span)),
                self.kind(TokenKind::ExclRange)
                    .map(|t: Token| (false, t.span)),
            ));

            let range_without_lo = range_op.clone().then(logical_or.clone().or_not()).map(
                |((inclusive, op_span), hi)| {
                    let span = match &hi {
                        Some(h) => op_span.merge(h.span),
                        None => op_span,
                    };
                    Expr {
                        kind: ExprKind::Range {
                            lo: None,
                            hi: hi.map(Box::new),
                            inclusive,
                        },
                        span,
                    }
                },
            );

            let range_with_lo = logical_or
                .clone()
                .then(range_op.then(logical_or.clone().or_not()).or_not())
                .map(|(lo, rest)| match rest {
                    None => lo,
                    Some(((inclusive, op_span), hi)) => {
                        let lo_span = lo.span;
                        let span = match &hi {
                            Some(h) => lo_span.merge(h.span),
                            None => lo_span.merge(op_span),
                        };
                        Expr {
                            kind: ExprKind::Range {
                                lo: Some(Box::new(lo)),
                                hi: hi.map(Box::new),
                                inclusive,
                            },
                            span,
                        }
                    }
                });

            choice((range_without_lo, range_with_lo)).boxed()
        };
        expr.define(expr_body);

        let block_body = {
            let expr = expr.clone();
            let block = block.clone();
            let type_p = self.type_parser_with_expr(expr.clone().boxed());

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

            let for_stmt = self
                .kind(TokenKind::ForKw)
                .then(pattern.clone())
                .then_ignore(self.kind(TokenKind::InKw))
                .then(expr.clone())
                .then(block.clone())
                .map(|(((for_tok, name), iter), body)| {
                    let span = for_tok.span.merge(body.span);
                    Stmt {
                        kind: StmtKind::For { name, iter, body },
                        span,
                    }
                })
                .boxed();

            let break_stmt = self
                .kind(TokenKind::BreakKw)
                .then(self.kind(TokenKind::Semicolon))
                .map(|(break_tok, semi_tok)| {
                    let span = break_tok.span.merge(semi_tok.span);
                    Stmt {
                        kind: StmtKind::Break,
                        span,
                    }
                });

            let continue_stmt = self
                .kind(TokenKind::ContinueKw)
                .then(self.kind(TokenKind::Semicolon))
                .map(|(continue_tok, semi_tok)| {
                    let span = continue_tok.span.merge(semi_tok.span);
                    Stmt {
                        kind: StmtKind::Continue,
                        span,
                    }
                });

            let return_stmt = self
                .kind(TokenKind::ReturnKw)
                .then(expr.clone())
                .then(self.kind(TokenKind::Semicolon))
                .map(|((ret_tok, value), semi_tok)| {
                    let span = ret_tok.span.merge(semi_tok.span);
                    Stmt {
                        kind: StmtKind::Return { ret: value },
                        span,
                    }
                })
                .boxed();

            let defer_stmt = self
                .kind(TokenKind::DeferKw)
                .then(expr.clone())
                .then(self.kind(TokenKind::Semicolon))
                .map(|((ret_tok, value), semi_tok)| {
                    let span = ret_tok.span.merge(semi_tok.span);
                    Stmt {
                        kind: StmtKind::Defer { defer: value },
                        span,
                    }
                })
                .boxed();

            let decl_stmt = self
                .kind(TokenKind::LetKw)
                .then(self.kind(TokenKind::MutKw).or_not())
                .then(pattern.clone())
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
                        name,
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

            let lend_decl = pattern
                .clone()
                .then(
                    self.kind(TokenKind::Colon)
                        .ignore_then(type_p.clone())
                        .or_not(),
                )
                .then_ignore(self.kind(TokenKind::Equals))
                .then(expr.clone())
                .map(|((name, ty), value)| {
                    let span = name.span.merge(value.span);
                    WithStmtLend {
                        name,
                        ty,
                        expr: value,
                        span,
                    }
                })
                .boxed();

            let with_stmt = self
                .kind(TokenKind::WithKw)
                .then(
                    lend_decl
                        .separated_by(self.kind(TokenKind::Comma))
                        .at_least(1)
                        .collect::<Vec<_>>(),
                )
                .then(block.clone())
                .map(|((with_tok, lends), body)| {
                    let span = with_tok.span.merge(body.span);
                    Stmt {
                        kind: StmtKind::With { lends, body },
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

            // Recover from a malformed statement by skipping to the next token that plausibly
            // starts a new statement, or to the block's closing `}` (left unconsumed, so the
            // enclosing `.then(CloseBrace)` below still sees it), producing `StmtKind::Error`.
            let stmt_start = choice((
                self.kind(TokenKind::WhileKw).ignored(),
                self.kind(TokenKind::ForKw).ignored(),
                self.kind(TokenKind::BreakKw).ignored(),
                self.kind(TokenKind::ContinueKw).ignored(),
                self.kind(TokenKind::ReturnKw).ignored(),
                self.kind(TokenKind::DeferKw).ignored(),
                self.kind(TokenKind::LetKw).ignored(),
                self.kind(TokenKind::WithKw).ignored(),
                self.kind(TokenKind::CloseBrace).ignored(),
            ));

            // Recovery must not fire when there's nothing actually broken left to parse: either
            // we're already sitting at `}` (`stmt.repeated()` legitimately has no more
            // statements), or what's ahead is a semicolon-less tail expression immediately
            // followed by `}` (the block's value, not a statement — see below). Forcing a skip
            // in either case would eat tokens that a later part of the grammar still needs, so we
            // only fall into the skip-at-least-one-token recovery once both checks fail.
            let at_terminal_position = choice((
                self.kind(TokenKind::CloseBrace).ignored(),
                expr.clone()
                    .then(self.kind(TokenKind::CloseBrace).ignored())
                    .ignored(),
            ))
            .rewind();

            let stmt_recovery = at_terminal_position
                .not()
                .ignore_then(self.recover_to_boundary(
                    stmt_start,
                    Stmt {
                        kind: StmtKind::Error,
                        span: SrcSpan::new(0, 0),
                    },
                ));

            let stmt = choice((
                while_stmt,
                for_stmt,
                break_stmt,
                continue_stmt,
                return_stmt,
                defer_stmt,
                decl_stmt,
                with_stmt,
                expr_stmt,
            ))
            .recover_with(via_parser(stmt_recovery))
            .boxed();

            // A block's final statement may be a bare expression with no trailing `;` — that's
            // the block's value when the block is used as an expression (`if`/`match`/`spawn`/
            // `concurrent` bodies, or a bare `{ ... }`). It's still just a `StmtKind::Expr` in
            // the tree; there's no separate "tail expr" slot on `Block`.
            self.kind(TokenKind::OpenBrace)
                .then(stmt.repeated().collect::<Vec<_>>())
                .then(expr.clone().or_not())
                .then(self.kind(TokenKind::CloseBrace))
                .map(|(((open_tok, mut stmts), tail), close_tok)| {
                    if let Some(tail) = tail {
                        let span = tail.span;
                        stmts.push(Stmt {
                            kind: StmtKind::Expr(tail),
                            span,
                        });
                    }
                    Block {
                        stmts,
                        span: open_tok.span.merge(close_tok.span),
                    }
                })
                .boxed()
        };
        block.define(block_body);

        (expr.boxed(), block.boxed())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::PatternKind;
    use crate::ast::interner::Interner;
    use crate::diag::DiagCtx;
    use crate::driver::src_map::SrcMap;
    use crate::lexer::Lexer;

    fn parse_expr(src: &str) -> Expr {
        DiagCtx::clear();
        Interner::clear();
        let chars: Vec<char> = src.chars().collect();
        let offset = SrcMap::add_file("<test>".to_string(), chars.clone());
        let tokens = Lexer::new(&chars, offset).tokenize();
        let parser = Parser::new(tokens.clone(), offset);
        let (output, errors) = parser.expr_parser().parse(&tokens[..]).into_output_errors();
        assert!(
            errors.is_empty(),
            "unexpected parse errors for {src:?}: {errors:?}"
        );
        output.expect("expected a successfully parsed expr")
    }

    #[test]
    fn parses_immutable_borrow_expr() {
        let expr = parse_expr("&x");
        match &expr.kind {
            ExprKind::Borrow {
                mutability,
                operand,
            } => {
                assert!(matches!(mutability, Mutability::Immutable));
                assert!(matches!(operand.kind, ExprKind::DeclRef(_)));
            }
            other => panic!("expected a borrow expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_mutable_borrow_expr() {
        let expr = parse_expr("&mut x");
        match &expr.kind {
            ExprKind::Borrow { mutability, .. } => {
                assert!(matches!(mutability, Mutability::Mutable));
            }
            other => panic!("expected a borrow expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_borrow_of_negated_expr() {
        // `&-x` exercises stacking a borrow prefix on top of a unary prefix.
        let expr = parse_expr("&-x");
        match &expr.kind {
            ExprKind::Borrow { operand, .. } => {
                assert!(matches!(
                    operand.kind,
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
            }
            other => panic!("expected a borrow expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_self_expr() {
        let expr = parse_expr("self");
        match &expr.kind {
            ExprKind::DeclRef(path) => {
                assert_eq!(Interner::resolve(path.segments[0].text), "self")
            }
            other => panic!("expected a decl-ref expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_field_access() {
        let expr = parse_expr("self.x");
        match &expr.kind {
            ExprKind::Field { base, field } => {
                assert!(matches!(base.kind, ExprKind::DeclRef(_)));
                assert_eq!(Interner::resolve(field.text), "x");
            }
            other => panic!("expected a field access expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_method_call() {
        let expr = parse_expr("self.dot(other)");
        match &expr.kind {
            ExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                assert!(matches!(receiver.kind, ExprKind::DeclRef(_)));
                assert_eq!(Interner::resolve(method.text), "dot");
                assert_eq!(args.len(), 1);
            }
            other => panic!("expected a method call expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_chained_field_and_method_postfix() {
        // `a.b.c(1)` exercises chaining field access into a method call.
        let expr = parse_expr("a.b.c(1)");
        match &expr.kind {
            ExprKind::MethodCall {
                receiver,
                method,
                args,
            } => {
                assert_eq!(Interner::resolve(method.text), "c");
                assert_eq!(args.len(), 1);
                match &receiver.kind {
                    ExprKind::Field { field, .. } => {
                        assert_eq!(Interner::resolve(field.text), "b")
                    }
                    other => panic!("expected a field access expr, got {other:?}"),
                }
            }
            other => panic!("expected a method call expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_index_expr() {
        let expr = parse_expr("a[0]");
        match &expr.kind {
            ExprKind::Index { base, index } => {
                assert!(matches!(base.kind, ExprKind::DeclRef(_)));
                assert!(matches!(index.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected an index expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_try_expr() {
        let expr = parse_expr("read_config()?");
        match &expr.kind {
            ExprKind::Try(inner) => assert!(matches!(inner.kind, ExprKind::FunCall { .. })),
            other => panic!("expected a try expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_postfix_binds_tighter_than_prefix() {
        // `-a.b` should be `-(a.b)`, not `(-a).b`.
        let expr = parse_expr("-a.b");
        match &expr.kind {
            ExprKind::Unary {
                op: UnaryOp::Neg,
                operand,
            } => {
                assert!(matches!(operand.kind, ExprKind::Field { .. }));
            }
            other => panic!("expected a unary expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_ctor_with_multiple_fields() {
        let expr = parse_expr("Vector2D { x: 1.0, y: 2.0 }");
        match &expr.kind {
            ExprKind::Ctor { path, payload } => {
                assert_eq!(Interner::resolve(path.segments[0].text), "Vector2D");
                assert_eq!(payload.len(), 2);
                assert_eq!(Interner::resolve(payload[0].name.text), "x");
                assert_eq!(Interner::resolve(payload[1].name.text), "y");
            }
            other => panic!("expected a ctor expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_ctor_with_nested_ctor_field() {
        // A ctor field's value can itself be another ctor expr.
        let expr = parse_expr("Line { start: Point { x: 0, y: 0 } }");
        match &expr.kind {
            ExprKind::Ctor { payload, .. } => {
                assert_eq!(payload.len(), 1);
                assert!(matches!(payload[0].expr.kind, ExprKind::Ctor { .. }));
            }
            other => panic!("expected a ctor expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_empty_tuple_expr() {
        let expr = parse_expr("()");
        assert!(matches!(expr.kind, ExprKind::Tuple(ref v) if v.is_empty()));
    }

    #[test]
    fn parses_grouping_not_tuple_for_single_element() {
        // `(x)` is a grouped expr, not a 1-tuple.
        let expr = parse_expr("(x)");
        assert!(matches!(expr.kind, ExprKind::DeclRef(_)));
    }

    #[test]
    fn parses_tuple_expr_with_multiple_elements() {
        let expr = parse_expr("(1, 2, 3)");
        match &expr.kind {
            ExprKind::Tuple(elems) => assert_eq!(elems.len(), 3),
            other => panic!("expected a tuple expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_nested_tuple_expr() {
        let expr = parse_expr("(1, (2, 3))");
        match &expr.kind {
            ExprKind::Tuple(elems) => {
                assert_eq!(elems.len(), 2);
                assert!(matches!(elems[1].kind, ExprKind::Tuple(_)));
            }
            other => panic!("expected a tuple expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_exclusive_range_expr() {
        let expr = parse_expr("0..5");
        match &expr.kind {
            ExprKind::Range {
                lo,
                hi,
                inclusive: false,
            } => {
                assert!(lo.is_some());
                assert!(hi.is_some());
            }
            other => panic!("expected a range expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_inclusive_range_expr() {
        let expr = parse_expr("0..=5");
        assert!(matches!(
            expr.kind,
            ExprKind::Range {
                inclusive: true,
                ..
            }
        ));
    }

    #[test]
    fn parses_range_without_lo() {
        let expr = parse_expr("..5");
        match &expr.kind {
            ExprKind::Range { lo, hi, .. } => {
                assert!(lo.is_none());
                assert!(hi.is_some());
            }
            other => panic!("expected a range expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_range_without_hi() {
        let expr = parse_expr("0..");
        match &expr.kind {
            ExprKind::Range { lo, hi, .. } => {
                assert!(lo.is_some());
                assert!(hi.is_none());
            }
            other => panic!("expected a range expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_full_range() {
        let expr = parse_expr("..");
        assert!(matches!(
            expr.kind,
            ExprKind::Range {
                lo: None,
                hi: None,
                ..
            }
        ));
    }

    #[test]
    fn parses_range_with_arithmetic_bounds() {
        // `a..b+1` should be `a..(b+1)` — range binds looser than `+`.
        let expr = parse_expr("a..b+1");
        match &expr.kind {
            ExprKind::Range { hi: Some(hi), .. } => {
                assert!(matches!(
                    hi.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            other => panic!("expected a range expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_expr_without_else() {
        let expr = parse_expr("if x < 5 { 1 }");
        match &expr.kind {
            ExprKind::If {
                cond,
                then_branch,
                else_branch,
            } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Lt,
                        ..
                    }
                ));
                assert_eq!(then_branch.stmts.len(), 1);
                assert!(else_branch.is_none());
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_else_expr() {
        let expr = parse_expr(r#"if x < 5 { "small" } else { "large" }"#);
        match &expr.kind {
            ExprKind::If { else_branch, .. } => {
                let else_branch = else_branch.as_ref().expect("expected an else branch");
                assert!(matches!(else_branch.kind, ExprKind::Block(_)));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_else_if_chain() {
        let expr = parse_expr("if a { 1 } else if b { 2 } else { 3 }");
        match &expr.kind {
            ExprKind::If { else_branch, .. } => {
                let else_branch = else_branch.as_ref().expect("expected an else branch");
                assert!(matches!(else_branch.kind, ExprKind::If { .. }));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_match_expr_with_multiple_arms() {
        let expr = parse_expr("match shape { Circle(r) => 1, Rectangle(w, h) => 2, _ => 0 }");
        match &expr.kind {
            ExprKind::Match { scrutinee, arms } => {
                assert!(matches!(scrutinee.kind, ExprKind::DeclRef(_)));
                assert_eq!(arms.len(), 3);
                assert!(matches!(arms[0].pat.kind, PatternKind::Ctor { .. }));
                assert!(matches!(arms[2].pat.kind, PatternKind::Wildcard));
            }
            other => panic!("expected a match expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_spawn_expr() {
        let expr = parse_expr("spawn { compute() }");
        match &expr.kind {
            ExprKind::Spawn(body) => assert_eq!(body.stmts.len(), 1),
            other => panic!("expected a spawn expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_concurrent_expr() {
        let expr = parse_expr("concurrent { let x = spawn { 1 }; x }");
        match &expr.kind {
            ExprKind::Concurrent(body) => assert_eq!(body.stmts.len(), 2),
            other => panic!("expected a concurrent expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_bare_block_expr() {
        let expr = parse_expr("{ let x = 1; x }");
        match &expr.kind {
            ExprKind::Block(block) => assert_eq!(block.stmts.len(), 2),
            other => panic!("expected a block expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_deeply_nested_expression() {
        // Exercises several layers together: call args containing a ctor, whose field is a
        // method call on an indexed, borrowed receiver.
        let expr = parse_expr("render(Frame { pixels: (&buf)[0].to_owned() })");
        match &expr.kind {
            ExprKind::FunCall { args, .. } => {
                assert_eq!(args.len(), 1);
                match &args[0].kind {
                    ExprKind::Ctor { payload, .. } => {
                        assert_eq!(payload.len(), 1);
                        assert!(matches!(payload[0].expr.kind, ExprKind::MethodCall { .. }));
                    }
                    other => panic!("expected a ctor expr, got {other:?}"),
                }
            }
            other => panic!("expected a call expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_closure_with_untyped_params_and_expr_body() {
        let expr = parse_expr("|x, y| x + y");
        match &expr.kind {
            ExprKind::Closure { params, ret, body } => {
                assert_eq!(params.len(), 2);
                assert_eq!(Interner::resolve(params[0].name.text), "x");
                assert!(params[0].ty.is_none());
                assert!(ret.is_none());
                assert!(matches!(body.kind, ExprKind::Binary { .. }));
            }
            other => panic!("expected a closure expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_closure_with_typed_params_return_type_and_block_body() {
        let expr = parse_expr("|x: i32, y: i32| -> i32 { x + y }");
        match &expr.kind {
            ExprKind::Closure { params, ret, body } => {
                assert_eq!(params.len(), 2);
                assert!(params[0].ty.is_some());
                assert!(params[1].ty.is_some());
                assert!(ret.is_some());
                assert!(matches!(body.kind, ExprKind::Block(_)));
            }
            other => panic!("expected a closure expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_closure_with_no_params() {
        // `||` lexes as one `DoublePipe` token, not two `Pipe`s.
        let expr = parse_expr("|| 42");
        match &expr.kind {
            ExprKind::Closure { params, body, .. } => {
                assert!(params.is_empty());
                assert!(matches!(body.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected a closure expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_closure_with_single_param_no_parens() {
        let expr = parse_expr("|x| x");
        match &expr.kind {
            ExprKind::Closure { params, .. } => assert_eq!(params.len(), 1),
            other => panic!("expected a closure expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_closure_passed_as_call_argument() {
        // Exercises a closure nested inside a call, matching how it's actually used in practice.
        let expr = parse_expr("map(xs, |x| x * 2)");
        match &expr.kind {
            ExprKind::FunCall { args, .. } => {
                assert_eq!(args.len(), 2);
                assert!(matches!(args[1].kind, ExprKind::Closure { .. }));
            }
            other => panic!("expected a call expr, got {other:?}"),
        }
    }
}
