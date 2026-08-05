//! Parses expressions, blocks, and statements.
//!
//! Expression precedence goes from tightest to loosest as the file reads top to bottom: postfix
//! operators, then prefix operators, then the binary operators in the usual arithmetic order,
//! then ranges, then assignment.
//!
//! [`BraceForms`] controls whether a bare `{` after an expression opens a struct literal or a
//! record payload. Condition and scrutinee positions (`if cond { ... }`, `match x { ... }`) turn
//! this off, so the `{` there always starts the following block instead.

use chumsky::Parser as ChumskyParser;
use chumsky::prelude::*;
use chumsky::recursive::Indirect;

use crate::ast::{
    AccessArgs, Arm, BinaryOp, Block, ClosureParam, Expr, ExprKind, Ident, Literal, Mutability,
    Path, Payload, PayloadField, Stmt, StmtKind, UnaryOp, WithLend,
};

use crate::ast::interner::Interner;
use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::token::{Token, TokenKind};

use super::{BoxedP, Extra, Parser};

type ExprRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Expr, Extra<'a>>>;
type BlockRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Block, Extra<'a>>>;

/// Whether a bare `{` after an expression may open a struct literal or record payload.
///
/// `Deny` is used for condition and scrutinee positions, where a `{` must instead start the
/// following block (e.g. `if Foo { x }` treats `Foo` as a value, not a struct literal).
#[derive(Clone, Copy, PartialEq, Eq)]
enum BraceForms {
    Allow,
    Deny,
}

impl Parser {
    /// Parses a single expression.
    pub fn expr_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        self.expr_and_block_parsers().0
    }

    /// Builds the mutually recursive expression and block parsers together, returning
    /// `(expr_parser, block_parser)`.
    ///
    /// They have to be built together because each recurses into the other: an expression can
    /// hold a block, and a block's statements can hold expressions.
    pub(crate) fn expr_and_block_parsers<'a>(&'a self) -> (BoxedP<'a, Expr>, BoxedP<'a, Block>) {
        let mut expr: ExprRec<'a> = Recursive::declare();
        // `expr` with brace forms denied. Used for condition and scrutinee positions.
        let mut expr_ns: ExprRec<'a> = Recursive::declare();
        let mut block: BlockRec<'a> = Recursive::declare();

        let path = self.path_parser();
        let ident = self.ident_parser();
        let pattern = self.pattern_parser();

        // The else branch is used for both statements (let-else) and exprs
        // (if expr)
        let else_expr = self
            .kind(TokenKind::ElseKw)
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
            .or_not()
            .boxed();

        let expr_body = |braces: BraceForms| {
            let expr = expr.clone();
            let expr_ns = expr_ns.clone();
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

            // Uses `self` as a value, e.g. the `self` in `self.x` inside a method body.
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
                        kind: ExprKind::Path(Path {
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
                        kind: ExprKind::Call {
                            callee: Box::new(Expr {
                                kind: ExprKind::Path(callee_path),
                                span: callee_span,
                            }),
                            args,
                        },
                        span,
                    }
                })
                .boxed();

            // This parses one field of a `Path { field: expr, ... }` struct literal.
            let ctor_field = ident
                .clone()
                .then_ignore(self.kind(TokenKind::Colon))
                .then(expr.clone())
                .map(|(name, value)| {
                    let span = name.span.merge(value.span);
                    PayloadField {
                        name,
                        value: Some(value),
                        span,
                    }
                })
                .boxed();

            let ctor_fields = ctor_field
                .clone()
                .separated_by(self.kind(TokenKind::Comma))
                .allow_trailing()
                .collect::<Vec<_>>()
                .boxed();

            // This parses a struct literal, `Path { field: expr, ... }`, but only when
            // `BraceForms::Allow`.
            let ctor = match braces {
                BraceForms::Allow => path
                    .clone()
                    .then_ignore(self.kind(TokenKind::OpenBrace))
                    .then(ctor_fields.clone())
                    .then(self.kind(TokenKind::CloseBrace))
                    .map(|((ctor_path, payload), close_tok)| {
                        let span = ctor_path.span.merge(close_tok.span);
                        Expr {
                            kind: ExprKind::Ctor {
                                path: Some(ctor_path),
                                payload,
                            },
                            span,
                        }
                    })
                    .boxed(),
                BraceForms::Deny => self.never(),
            };

            // This parses `.{ x: 1.0, y: 2.0 }`, a struct literal with the type elided.
            let elided_ctor = self
                .kind(TokenKind::Period)
                .then_ignore(self.kind(TokenKind::OpenBrace))
                .then(ctor_fields)
                .then(self.kind(TokenKind::CloseBrace))
                .map(|((dot_tok, payload), close_tok)| Expr {
                    kind: ExprKind::Ctor {
                        path: None,
                        payload,
                    },
                    span: dot_tok.span.merge(close_tok.span),
                })
                .boxed();

            // A record payload's fields look like `{ l: 4.0 }`, or `{ l }` as shorthand for
            // `{ l: l }`. This mirrors the pattern side.
            let record_payload = self
                .kind(TokenKind::OpenBrace)
                .ignore_then(
                    ident
                        .clone()
                        .then(
                            self.kind(TokenKind::Colon)
                                .ignore_then(expr.clone())
                                .or_not(),
                        )
                        .map(|(name, value)| {
                            let span = match &value {
                                Some(e) => name.span.merge(e.span),
                                None => name.span,
                            };
                            PayloadField { name, value, span }
                        })
                        .separated_by(self.kind(TokenKind::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then(self.kind(TokenKind::CloseBrace))
                .boxed();

            // This parses the record-shaped payload of a variant, e.g. the `{ l: 4.0 }` in
            // `.square { l: 4.0 }`.
            let record_variant_payload = match braces {
                BraceForms::Allow => record_payload
                    .clone()
                    .map(|(fields, close_tok)| (Payload::Record(fields), close_tok.span))
                    .boxed(),
                BraceForms::Deny => self.never(),
            };

            let variant_payload = choice((
                self.kind(TokenKind::OpenParen)
                    .ignore_then(expr.clone())
                    .then(self.kind(TokenKind::CloseParen))
                    .map(|(value, close_tok)| (Payload::Single(Box::new(value)), close_tok.span)),
                record_variant_payload,
            ))
            .boxed();

            let variant = self
                .kind(TokenKind::Period)
                .then(ident.clone())
                .then(variant_payload.or_not())
                .map(|((dot_tok, variant), payload)| {
                    let (payload, span) = match payload {
                        Some((payload, close_span)) => (payload, dot_tok.span.merge(close_span)),
                        None => (Payload::None, dot_tok.span.merge(variant.span)),
                    };
                    Expr {
                        kind: ExprKind::Variant { variant, payload },
                        span,
                    }
                })
                .boxed();

            let decl_ref = path.clone().map(|p: Path| {
                let span = p.span;
                Expr {
                    kind: ExprKind::Path(p),
                    span,
                }
            });

            // `(expr)` is a grouped expression. `(expr, expr, ...)` with zero or at least two
            // elements is a tuple.
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

            // A bare `{ ... }` block can also stand alone as an expression.
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

            // Parses `if let pat = scrutinee { .. }`.
            let if_let_expr = self
                .kind(TokenKind::IfKw)
                .then_ignore(self.kind(TokenKind::LetKw))
                .then(pattern.clone())
                .then_ignore(self.kind(TokenKind::Equals))
                .then(expr_ns.clone())
                .then(block.clone())
                .then(else_expr.clone())
                .map(|((((if_tok, pat), scrutinee), then_block), else_expr)| {
                    let span = match &else_expr {
                        Some(e) => if_tok.span.merge(e.span),
                        None => if_tok.span.merge(then_block.span),
                    };
                    Expr {
                        kind: ExprKind::IfLet {
                            pat,
                            scrutinee: Box::new(scrutinee),
                            then_block,
                            else_expr: else_expr.map(Box::new),
                        },
                        span,
                    }
                })
                .boxed();

            let if_expr = self
                .kind(TokenKind::IfKw)
                .then(expr_ns.clone())
                .then(block.clone())
                .then(else_expr.clone())
                .map(|(((if_tok, cond), then_block), else_expr)| {
                    let span = match &else_expr {
                        Some(e) => if_tok.span.merge(e.span),
                        None => if_tok.span.merge(then_block.span),
                    };
                    Expr {
                        kind: ExprKind::If {
                            cond: Box::new(cond),
                            then_block,
                            else_expr: else_expr.map(Box::new),
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
                    Arm {
                        pat,
                        body: Box::new(body),
                        span,
                    }
                })
                .boxed();

            let match_expr = self
                .kind(TokenKind::MatchKw)
                .then(expr_ns.clone())
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

            // Closures look like `|x: i32, y: i32| -> i32 { x + y }`, `|x| x + 1`, or `|| 42`.
            // Parameter types and the return type are optional and get inferred later. The
            // body is any expr, so a `{ ... }` block body works the same as a bare expression.
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
            // spelled as two `Pipe` tokens. It needs its own case.
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
                if_let_expr,
                if_expr,
                match_expr,
                spawn_expr,
                concurrent_expr,
                call,
                ctor,
                // `elided_ctor` and `variant` both start with `.`. Try `elided_ctor` first: it
                // needs a `{` right after the `.`, while `variant` needs an identifier.
                elided_ctor,
                variant,
                self_expr,
                decl_ref,
                tuple_or_group,
                block_expr,
            ))
            .boxed();

            // Postfix operators (`.member`, `.member(args)`, `[index]`, `?`) bind tighter than
            // any prefix operator, so `-x.y` parses as `-(x.y)`.
            enum Postfix {
                Access(Ident, AccessArgs),
                Index(Expr),
                Try,
            }

            let access_record = match braces {
                BraceForms::Allow => record_payload
                    .clone()
                    .map(|(fields, close_tok)| (AccessArgs::Record(fields), close_tok.span))
                    .boxed(),
                BraceForms::Deny => self.never(),
            };

            let access_op = self
                .kind(TokenKind::Period)
                .ignore_then(ident.clone())
                .then(
                    choice((
                        self.kind(TokenKind::OpenParen)
                            .ignore_then(
                                expr.clone()
                                    .separated_by(self.kind(TokenKind::Comma))
                                    .allow_trailing()
                                    .collect::<Vec<_>>(),
                            )
                            .then(self.kind(TokenKind::CloseParen))
                            .map(|(args, close_tok)| (AccessArgs::Call(args), close_tok.span)),
                        access_record,
                    ))
                    .or_not(),
                )
                .map(|(name, args)| match args {
                    Some((args, close_span)) => (Postfix::Access(name, args), close_span),
                    None => {
                        let span = name.span;
                        (Postfix::Access(name, AccessArgs::None), span)
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

            let postfix_op = choice((access_op, index_op, try_op));

            let postfix = atom
                .foldl(postfix_op.repeated(), |receiver, (op, op_span)| {
                    let span = receiver.span.merge(op_span);
                    match op {
                        Postfix::Access(member, args) => Expr {
                            kind: ExprKind::Access {
                                base: Box::new(receiver),
                                member,
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
                .foldl(mul_op.then(unary.clone()).repeated(), Expr::binary)
                .boxed();

            let add_op = choice((
                bin_op(TokenKind::Plus, BinaryOp::Add),
                bin_op(TokenKind::Minus, BinaryOp::Sub),
            ));
            let sum = product
                .clone()
                .foldl(add_op.then(product.clone()).repeated(), Expr::binary)
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
                .foldl(cmp_op.then(sum.clone()).repeated(), Expr::binary)
                .boxed();

            let and_op = bin_op(TokenKind::DoubleAmp, BinaryOp::And);
            let logical_and = comparison
                .clone()
                .foldl(and_op.then(comparison.clone()).repeated(), Expr::binary)
                .boxed();

            let or_op = bin_op(TokenKind::DoublePipe, BinaryOp::Or);
            let logical_or = logical_and
                .clone()
                .foldl(or_op.then(logical_and.clone()).repeated(), Expr::binary)
                .boxed();

            // A range can look like `a..b`, `a..=b`, `a..`, `..b`, `..=b`, or `..`; either bound
            // is optional.
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

            let range = choice((range_without_lo, range_with_lo)).boxed();

            // Assignment (`place = value`, `place += value`, etc.) has the lowest precedence
            // of all.
            let assign_op = choice((
                self.kind(TokenKind::Equals).map(|t: Token| (None, t.span)),
                self.kind(TokenKind::PlusEquals)
                    .map(|t: Token| (Some(BinaryOp::Add), t.span)),
                self.kind(TokenKind::SubEquals)
                    .map(|t: Token| (Some(BinaryOp::Sub), t.span)),
                self.kind(TokenKind::MulEquals)
                    .map(|t: Token| (Some(BinaryOp::Mul), t.span)),
                self.kind(TokenKind::DivEquals)
                    .map(|t: Token| (Some(BinaryOp::Div), t.span)),
                self.kind(TokenKind::ModEquals)
                    .map(|t: Token| (Some(BinaryOp::Rem), t.span)),
            ));

            range
                .clone()
                .then(assign_op.then(expr.clone()).or_not())
                .map(|(lhs, rest)| match rest {
                    None => lhs,
                    Some(((op, _op_span), rhs)) => {
                        let span = lhs.span.merge(rhs.span);
                        let kind = match op {
                            None => ExprKind::Assign {
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                            Some(op) => ExprKind::AssignOp {
                                op,
                                lhs: Box::new(lhs),
                                rhs: Box::new(rhs),
                            },
                        };
                        Expr { kind, span }
                    }
                })
                .boxed()
        };
        let full = expr_body(BraceForms::Allow);
        let restricted = expr_body(BraceForms::Deny);
        expr.define(full);
        expr_ns.define(restricted);

        let block_body = {
            let expr = expr.clone();
            let expr_ns = expr_ns.clone();
            let block = block.clone();
            let type_p = self.type_parser_with_expr(expr.clone().boxed());

            // Parses `while let pat = scrutinee { .. }`.
            let while_let_stmt = self
                .kind(TokenKind::WhileKw)
                .then_ignore(self.kind(TokenKind::LetKw))
                .then(pattern.clone())
                .then_ignore(self.kind(TokenKind::Equals))
                .then(expr_ns.clone())
                .then(block.clone())
                .map(|(((while_tok, pat), scrutinee), block)| {
                    let span = while_tok.span.merge(block.span);
                    Stmt {
                        kind: StmtKind::WhileLet {
                            pat,
                            scrutinee,
                            block,
                        },
                        span,
                    }
                })
                .boxed();

            let while_stmt = self
                .kind(TokenKind::WhileKw)
                .then(expr_ns.clone())
                .then(block.clone())
                .map(|((while_tok, cond), block)| {
                    let span = while_tok.span.merge(block.span);
                    Stmt {
                        kind: StmtKind::While { cond, block },
                        span,
                    }
                })
                .boxed();

            let for_stmt = self
                .kind(TokenKind::ForKw)
                .then(pattern.clone())
                .then_ignore(self.kind(TokenKind::InKw))
                .then(expr_ns.clone())
                .then(block.clone())
                .map(|(((for_tok, pat), iter), block)| {
                    let span = for_tok.span.merge(block.span);
                    Stmt {
                        kind: StmtKind::For { pat, iter, block },
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
                        kind: StmtKind::Return(value),
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
                        kind: StmtKind::Defer(value),
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
                .then(
                    self.kind(TokenKind::ElseKw)
                        .ignore_then(block.clone())
                        .or_not(),
                )
                .then(self.kind(TokenKind::Semicolon))
                .map(
                    |((((((let_tok, mut_tok), name), ty), value), else_block), semi_tok)| {
                        let mutability = if mut_tok.is_some() {
                            Mutability::Mutable
                        } else {
                            Mutability::Immutable
                        };
                        let span = let_tok.span.merge(semi_tok.span);
                        Stmt {
                            kind: StmtKind::Let {
                                mutability,
                                pat: name,
                                ty,
                                init: value,
                                else_block,
                            },
                            span,
                        }
                    },
                )
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
                .map(|((pat, ty), value)| {
                    let span = pat.span.merge(value.span);
                    WithLend {
                        pat,
                        ty,
                        init: value,
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
                .map(|((with_tok, lends), block)| {
                    let span = with_tok.span.merge(block.span);
                    Stmt {
                        kind: StmtKind::With { lends, block },
                        span,
                    }
                })
                .boxed();

            let expr_stmt = expr
                .clone()
                .then(self.kind(TokenKind::Semicolon).or_not())
                .try_map(|(value, semi_tok), span| {
                    if semi_tok.is_none() && !value.kind.is_block_bodied() {
                        return Err(Rich::custom(
                            span,
                            "expected `;` after this expression statement",
                        ));
                    }
                    let span = match semi_tok {
                        Some(semi_tok) => value.span.merge(semi_tok.span),
                        None => value.span,
                    };
                    Ok(Stmt {
                        kind: StmtKind::Expr {
                            expr: value,
                            semi: semi_tok.is_some(),
                        },
                        span,
                    })
                })
                .boxed();

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

            // Recovery must not fire once the block is really at its end (just the closing
            // `}`, or a valid tail expression followed by `}`). Otherwise it would eat the
            // `}` or the tail expression as if they were part of a broken statement.
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
                while_let_stmt,
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

            self.kind(TokenKind::OpenBrace)
                .then(stmt.repeated().collect::<Vec<_>>())
                .then(expr.clone().or_not())
                .then(self.kind(TokenKind::CloseBrace))
                .map(|(((open_tok, mut stmts), tail), close_tok)| {
                    if let Some(tail) = tail {
                        let span = tail.span;
                        stmts.push(Stmt {
                            kind: StmtKind::Expr {
                                expr: tail,
                                semi: false,
                            },
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
    use crate::ast::PatKind;
    use crate::ast::interner::Interner;
    use crate::testing::lex_src;

    fn parse_expr(src: &str) -> Expr {
        let (tokens, _) = lex_src(src);
        let parser = Parser::new();
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
                assert!(matches!(operand.kind, ExprKind::Path(_)));
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
            ExprKind::Path(path) => {
                assert_eq!(Interner::resolve(path.segments[0].text), "self")
            }
            other => panic!("expected a decl-ref expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_field_access() {
        let expr = parse_expr("self.x");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert!(matches!(base.kind, ExprKind::Path(_)));
                assert_eq!(Interner::resolve(member.text), "x");
                assert!(matches!(args, AccessArgs::None));
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_method_call() {
        let expr = parse_expr("self.dot(other)");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert!(matches!(base.kind, ExprKind::Path(_)));
                assert_eq!(Interner::resolve(member.text), "dot");
                assert!(matches!(args, AccessArgs::Call(args) if args.len() == 1));
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_chained_access_postfix() {
        // `a.b.c(1)` exercises chaining one access into another.
        let expr = parse_expr("a.b.c(1)");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert_eq!(Interner::resolve(member.text), "c");
                assert!(matches!(args, AccessArgs::Call(args) if args.len() == 1));
                match &base.kind {
                    ExprKind::Access { member, args, .. } => {
                        assert_eq!(Interner::resolve(member.text), "b");
                        assert!(matches!(args, AccessArgs::None));
                    }
                    other => panic!("expected an access expr, got {other:?}"),
                }
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    /// This tests a variant named through its type, with a record payload. It is the one
    /// access shape the grammar pins down on its own, since neither a field nor a method has
    /// a brace form.
    #[test]
    fn parses_qualified_variant_with_record_payload() {
        let expr = parse_expr("Expr.int { value: 3 }");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert!(matches!(base.kind, ExprKind::Path(_)));
                assert_eq!(Interner::resolve(member.text), "int");
                match args {
                    AccessArgs::Record(fields) => {
                        assert_eq!(fields.len(), 1);
                        assert_eq!(Interner::resolve(fields[0].name.text), "value");
                    }
                    other => panic!("expected a record payload, got {other:?}"),
                }
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    // -----------------------------------------------------------------
    // Brace forms in condition position (`BraceForms::Deny`)
    // -----------------------------------------------------------------

    /// `if a.b { x }` must keep its body: the `{` is the block, not a record payload.
    #[test]
    fn record_payload_is_denied_in_condition_position() {
        let expr = parse_expr("if a.b { x } else { 0 }");
        match &expr.kind {
            ExprKind::If {
                cond, else_expr, ..
            } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Access {
                        args: AccessArgs::None,
                        ..
                    }
                ));
                assert!(else_expr.is_some());
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    /// This tests the same restriction on struct literals, the case that used to require
    /// parenthesizing.
    #[test]
    fn struct_literal_is_denied_in_condition_position() {
        let expr = parse_expr("if Foo { x } else { 0 }");
        match &expr.kind {
            ExprKind::If {
                cond,
                then_block,
                else_expr,
            } => {
                assert!(matches!(cond.kind, ExprKind::Path(_)));
                assert_eq!(then_block.stmts.len(), 1);
                assert!(else_expr.is_some());
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn brace_forms_are_denied_in_a_match_scrutinee() {
        let expr = parse_expr("match Foo { x => 1, _ => 0 }");
        match &expr.kind {
            ExprKind::Match { scrutinee, arms } => {
                assert!(matches!(scrutinee.kind, ExprKind::Path(_)));
                assert_eq!(arms.len(), 2);
            }
            other => panic!("expected a match expr, got {other:?}"),
        }
    }

    /// The restriction covers the whole top-level spine but stops at any bracketing, so a
    /// parenthesized or argument-position brace form is still fine in a condition.
    #[test]
    fn brace_forms_are_allowed_inside_brackets_in_a_condition() {
        let expr = parse_expr("if (Foo { a: 1 }).b { x } else { 0 }");
        match &expr.kind {
            ExprKind::If { cond, .. } => match &cond.kind {
                ExprKind::Access { base, args, .. } => {
                    assert!(matches!(base.kind, ExprKind::Ctor { .. }));
                    assert!(matches!(args, AccessArgs::None));
                }
                other => panic!("expected an access expr, got {other:?}"),
            },
            other => panic!("expected an if expr, got {other:?}"),
        }

        let expr = parse_expr("if f(Foo { a: 1 }) { x } else { 0 }");
        match &expr.kind {
            ExprKind::If { cond, .. } => match &cond.kind {
                ExprKind::Call { args, .. } => {
                    assert!(matches!(args[0].kind, ExprKind::Ctor { .. }));
                }
                other => panic!("expected a call expr, got {other:?}"),
            },
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    /// Forms that open with `.` are never ambiguous, since a block can't start with `.`.
    #[test]
    fn dot_prefixed_forms_survive_in_condition_position() {
        let expr = parse_expr("if .{ a: 1 } { x } else { 0 }");
        match &expr.kind {
            ExprKind::If { cond, .. } => {
                assert!(matches!(cond.kind, ExprKind::Ctor { path: None, .. }));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }

        let expr = parse_expr("if .none { x } else { 0 }");
        match &expr.kind {
            ExprKind::If { cond, .. } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Variant {
                        payload: Payload::None,
                        ..
                    }
                ));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_index_expr() {
        let expr = parse_expr("a[0]");
        match &expr.kind {
            ExprKind::Index { base, index } => {
                assert!(matches!(base.kind, ExprKind::Path(_)));
                assert!(matches!(index.kind, ExprKind::Literal(Literal::Int { .. })));
            }
            other => panic!("expected an index expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_try_expr() {
        let expr = parse_expr("read_config()?");
        match &expr.kind {
            ExprKind::Try(inner) => assert!(matches!(inner.kind, ExprKind::Call { .. })),
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
                assert!(matches!(operand.kind, ExprKind::Access { .. }));
            }
            other => panic!("expected a unary expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_ctor_with_multiple_fields() {
        let expr = parse_expr("Vector2D { x: 1.0, y: 2.0 }");
        match &expr.kind {
            ExprKind::Ctor { path, payload } => {
                let path = path.as_ref().expect("`Vector2D { .. }` names its type");
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
                assert!(matches!(
                    payload[0].value.as_ref().unwrap().kind,
                    ExprKind::Ctor { .. }
                ));
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
        assert!(matches!(expr.kind, ExprKind::Path(_)));
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
        // `a..b+1` should be `a..(b+1)`, since range binds looser than `+`.
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
                then_block,
                else_expr,
            } => {
                assert!(matches!(
                    cond.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Lt,
                        ..
                    }
                ));
                assert_eq!(then_block.stmts.len(), 1);
                assert!(else_expr.is_none());
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_else_expr() {
        let expr = parse_expr(r#"if x < 5 { "small" } else { "large" }"#);
        match &expr.kind {
            ExprKind::If { else_expr, .. } => {
                let else_expr = else_expr.as_ref().expect("expected an else branch");
                assert!(matches!(else_expr.kind, ExprKind::Block(_)));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_else_if_chain() {
        let expr = parse_expr("if a { 1 } else if b { 2 } else { 3 }");
        match &expr.kind {
            ExprKind::If { else_expr, .. } => {
                let else_expr = else_expr.as_ref().expect("expected an else branch");
                assert!(matches!(else_expr.kind, ExprKind::If { .. }));
            }
            other => panic!("expected an if expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_let() {
        let expr = parse_expr("if let .some(x) = o { x } else { 0 }");
        match &expr.kind {
            ExprKind::IfLet {
                pat,
                scrutinee,
                else_expr,
                ..
            } => {
                assert!(matches!(pat.kind, PatKind::Variant { .. }));
                assert!(matches!(scrutinee.kind, ExprKind::Path(_)));
                assert!(else_expr.is_some());
            }
            other => panic!("expected an if-let expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_if_let_without_else() {
        let expr = parse_expr("if let .some(x) = o { x }");
        match &expr.kind {
            ExprKind::IfLet { else_expr, .. } => assert!(else_expr.is_none()),
            other => panic!("expected an if-let expr, got {other:?}"),
        }
    }

    /// `else` after an `if let` takes the same branch parser as a plain `if`, so chaining works.
    #[test]
    fn parses_else_if_let_chain() {
        let expr = parse_expr("if let .some(a) = o { a } else if let .none = o { 1 } else { 2 }");
        match &expr.kind {
            ExprKind::IfLet { else_expr, .. } => {
                let else_expr = else_expr.as_ref().expect("expected an else branch");
                assert!(matches!(else_expr.kind, ExprKind::IfLet { .. }));
            }
            other => panic!("expected an if-let expr, got {other:?}"),
        }
    }

    /// The scrutinee sits in condition position, so brace forms are denied there too.
    #[test]
    fn if_let_scrutinee_denies_brace_forms() {
        let expr = parse_expr("if let .some(x) = Foo { x } else { 0 }");
        match &expr.kind {
            ExprKind::IfLet {
                scrutinee,
                then_block,
                ..
            } => {
                assert!(matches!(scrutinee.kind, ExprKind::Path(_)));
                assert_eq!(then_block.stmts.len(), 1);
            }
            other => panic!("expected an if-let expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_match_expr_with_multiple_arms() {
        let expr = parse_expr("match shape { .circle(r) => 1, .rectangle((w, h)) => 2, _ => 0 }");
        match &expr.kind {
            ExprKind::Match { scrutinee, arms } => {
                assert!(matches!(scrutinee.kind, ExprKind::Path(_)));
                assert_eq!(arms.len(), 3);
                assert!(matches!(arms[0].pat.kind, PatKind::Variant { .. }));
                assert!(matches!(arms[2].pat.kind, PatKind::Wildcard));
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
            ExprKind::Call { args, .. } => {
                assert_eq!(args.len(), 1);
                match &args[0].kind {
                    ExprKind::Ctor { payload, .. } => {
                        assert_eq!(payload.len(), 1);
                        assert!(matches!(
                            payload[0].value.as_ref().unwrap().kind,
                            ExprKind::Access { .. }
                        ));
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
            ExprKind::Call { args, .. } => {
                assert_eq!(args.len(), 2);
                assert!(matches!(args[1].kind, ExprKind::Closure { .. }));
            }
            other => panic!("expected a call expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_plain_assignment() {
        let expr = parse_expr("i = i + 1");
        match &expr.kind {
            ExprKind::Assign { lhs, rhs } => {
                assert!(matches!(lhs.kind, ExprKind::Path(_)));
                assert!(matches!(
                    rhs.kind,
                    ExprKind::Binary {
                        op: BinaryOp::Add,
                        ..
                    }
                ));
            }
            other => panic!("expected an assign expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_compound_assignment_operators() {
        for (src, op) in [
            ("x += 1", BinaryOp::Add),
            ("x -= 1", BinaryOp::Sub),
            ("x *= 1", BinaryOp::Mul),
            ("x /= 1", BinaryOp::Div),
            ("x %= 1", BinaryOp::Rem),
        ] {
            let expr = parse_expr(src);
            match &expr.kind {
                ExprKind::AssignOp { op: got, lhs, .. } => {
                    assert_eq!(*got, op, "wrong op for {src:?}");
                    assert!(matches!(lhs.kind, ExprKind::Path(_)));
                }
                other => panic!("expected an assign-op expr for {src:?}, got {other:?}"),
            }
        }
    }

    #[test]
    fn assignment_is_right_associative() {
        // `a = b = c` should be `a = (b = c)`, not `(a = b) = c`.
        let expr = parse_expr("a = b = c");
        match &expr.kind {
            ExprKind::Assign { rhs, .. } => {
                assert!(matches!(rhs.kind, ExprKind::Assign { .. }));
            }
            other => panic!("expected an assign expr, got {other:?}"),
        }
    }

    #[test]
    fn assignment_binds_looser_than_range() {
        // `x = a..b` should parse the whole range as the RHS, not `(x = a)..b`.
        let expr = parse_expr("x = a..b");
        match &expr.kind {
            ExprKind::Assign { rhs, .. } => {
                assert!(matches!(rhs.kind, ExprKind::Range { .. }));
            }
            other => panic!("expected an assign expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_assignment_through_a_field_place() {
        // The left-hand side of an assignment need not be a bare name.
        let expr = parse_expr("point.x = 1");
        match &expr.kind {
            ExprKind::Assign { lhs, .. } => {
                assert!(matches!(lhs.kind, ExprKind::Access { .. }));
            }
            other => panic!("expected an assign expr, got {other:?}"),
        }
    }
}
