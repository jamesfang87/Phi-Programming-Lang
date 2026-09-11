use chumsky::Parser as ChumskyParser;
use chumsky::input::InputRef;
use chumsky::prelude::*;
use chumsky::recursive::Indirect;

use crate::ast::{
    AccessArgs, Arm, BinaryOp, Block, ClosureParam, Expr, ExprKind, Ident, Literal, Mutability,
    NodeId, Pat, Path, Payload, PayloadField, Stmt, StmtKind, Ty, UnaryOp, WithLend,
};

use crate::ast::interner::Interner;
use crate::driver::source::SrcSpan;
use crate::lexer::literal::strip_digit_separators;
use crate::lexer::token::{Token, TokenKind};

use super::{
    BoxedP, BraceForms, Extra, Parser, recovery::STATEMENT_RECOVERY, recovery::recover_by_skipping,
};

/// Desugars range literals (ex: `lo..hi`) into explict construction (ex: `std::range::Range { .. }`)
fn desugar_range(lo: Option<Expr>, hi: Option<Expr>, inclusive: bool, span: SrcSpan) -> Expr {
    let field = |name: &str, value: Expr| PayloadField {
        id: NodeId::next(),
        name: Ident {
            text: Interner::intern(name),
            span,
        },
        span: span.merge(value.span),
        value: Some(value),
    };
    let lo_span = lo.as_ref().map_or(span, |e| e.span);
    let hi_span = hi.as_ref().map_or(span, |e| e.span);
    let path = Path {
        segments: ["std", "range", "Range"]
            .into_iter()
            .map(|segment| Ident {
                text: Interner::intern(segment),
                span,
            })
            .collect(),
        span,
    };
    Expr {
        id: NodeId::next(),
        kind: ExprKind::Ctor {
            path: Some(path),
            payload: vec![
                field("left", Expr::range_bound_variant(lo, lo_span)),
                field("right", Expr::range_bound_variant(hi, hi_span)),
                field(
                    "inclusive",
                    Expr {
                        id: NodeId::next(),
                        kind: ExprKind::Literal(Literal::Bool(inclusive)),
                        span,
                    },
                ),
            ],
        },
        span,
    }
}

type ExprRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Expr, Extra<'a>>>;
type BlockRec<'a> = Recursive<Indirect<'a, 'a, &'a [Token], Block, Extra<'a>>>;
impl Parser {
    /// Parses a single expression.
    pub fn expr_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        self.expr_and_block_parsers().0
    }

    pub(crate) fn literal_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        choice((
            self.kind(TokenKind::IntLiteral).map(Expr::int),
            self.kind(TokenKind::FloatLiteral).map(Expr::float),
            self.kind(TokenKind::StrLiteral).map(Expr::string),
            self.kind(TokenKind::CharLiteral).map(Expr::char),
            self.kind(TokenKind::TrueKw).map(Expr::bool_literal(true)),
            self.kind(TokenKind::FalseKw).map(Expr::bool_literal(false)),
        ))
        .boxed()
    }

    fn self_expr_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        self.kind(TokenKind::LowerSelfKw)
            .map(|t: Token| {
                let name = Ident::of_token(t);
                Expr::new(
                    ExprKind::Path(Path {
                        segments: vec![name],
                        span: t.span,
                    }),
                    t.span,
                )
            })
            .boxed()
    }

    fn self_kw_parser<'a>(&'a self) -> BoxedP<'a, Expr> {
        self.kind(TokenKind::UpperSelfKw)
            .map(|t: Token| Expr::new(ExprKind::SelfKw, t.span))
            .boxed()
    }

    pub(crate) fn expr_and_block_parsers<'a>(&'a self) -> (BoxedP<'a, Expr>, BoxedP<'a, Block>) {
        let mut expr: ExprRec<'a> = Recursive::declare();
        let mut expr_without_brace_forms: ExprRec<'a> = Recursive::declare();
        let mut block: BlockRec<'a> = Recursive::declare();

        let grammar = Grammar {
            parser: self,
            expr: expr.clone(),
            expr_without_brace_forms: expr_without_brace_forms.clone(),
            block: block.clone(),
            path: self.path_parser(),
            ident: self.ident_parser(),
            pattern: self.pattern_parser(),
        };

        expr.define(
            grammar
                .expression_parser(BraceForms::Allow)
                .labelled("an expression"),
        );
        expr_without_brace_forms.define(
            grammar
                .expression_parser(BraceForms::Deny)
                .labelled("an expression"),
        );
        block.define(grammar.block_parser().labelled("a block"));

        (expr.boxed(), block.boxed())
    }
}

struct Grammar<'a> {
    parser: &'a Parser,
    expr: ExprRec<'a>,
    expr_without_brace_forms: ExprRec<'a>,
    block: BlockRec<'a>,
    path: BoxedP<'a, Path>,
    ident: BoxedP<'a, Ident>,
    pattern: BoxedP<'a, Pat>,
}

enum Postfix {
    Access(Ident, AccessArgs),
    TupleFieldPair(Ident, Ident),
    Index(Expr),
    Try,
}

enum Prefix {
    Unary(UnaryOp),
    Borrow(Mutability),
}

impl<'a> Grammar<'a> {
    /// Turns off one alternative of a `choice` without changing the choice's shape.
    fn never<O: 'a>(&self) -> BoxedP<'a, O> {
        any()
            .filter(|_: &Token| false)
            .map(|_| unreachable!("`never` matches nothing, so nothing is ever mapped"))
            .boxed()
    }

    fn brace_gated<O: 'a>(&self, braces: BraceForms, parser: BoxedP<'a, O>) -> BoxedP<'a, O> {
        match braces {
            BraceForms::Allow => parser,
            BraceForms::Deny => self.never(),
        }
    }

    fn type_parser(&self) -> BoxedP<'a, Ty> {
        self.parser.type_parser_with_expr(self.expr.clone().boxed())
    }

    fn else_expr(&self) -> BoxedP<'a, Option<Expr>> {
        self.parser
            .kind(TokenKind::ElseKw)
            .ignore_then(choice((
                self.block.clone().map(|b: Block| {
                    let span = b.span;
                    Expr::new(ExprKind::Block(b), span)
                }),
                self.expr.clone(),
            )))
            .or_not()
            .boxed()
    }

    fn call_parser(&self) -> BoxedP<'a, Expr> {
        self.path
            .clone()
            .then_ignore(self.parser.kind(TokenKind::OpenParen))
            .then(
                self.expr
                    .clone()
                    .separated_by(self.parser.kind(TokenKind::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then(self.parser.kind(TokenKind::CloseParen))
            .map(|((callee_path, args), close_tok)| {
                let callee_span = callee_path.span;
                let span = callee_span.merge(close_tok.span);
                Expr::new(
                    ExprKind::Call {
                        callee: Box::new(Expr::new(ExprKind::Path(callee_path), callee_span)),
                        args,
                    },
                    span,
                )
            })
            .boxed()
    }

    fn builtin_expr_parser(&self) -> BoxedP<'a, Expr> {
        choice((
            self.parser
                .kind(TokenKind::Assert)
                .then_ignore(self.parser.kind(TokenKind::OpenParen))
                .then(self.expr.clone())
                .then(
                    self.parser
                        .kind(TokenKind::Comma)
                        .ignore_then(self.expr.clone())
                        .or_not(),
                )
                .then(self.parser.kind(TokenKind::CloseParen))
                .map(|(((assert_tok, cond), msg), close_tok)| {
                    let span = assert_tok.span.merge(close_tok.span);
                    Expr::new(
                        ExprKind::Assert {
                            cond: Box::new(cond),
                            msg: msg.map(Box::new),
                        },
                        span,
                    )
                }),
            self.parser
                .kind(TokenKind::Panic)
                .then_ignore(self.parser.kind(TokenKind::OpenParen))
                .then(self.expr.clone().or_not())
                .then(self.parser.kind(TokenKind::CloseParen))
                .map(|((panic_tok, msg), close_tok)| {
                    let span = panic_tok.span.merge(close_tok.span);
                    Expr::new(
                        ExprKind::Panic {
                            msg: msg.map(Box::new),
                        },
                        span,
                    )
                }),
            self.parser
                .kind(TokenKind::Unreachable)
                .then_ignore(self.parser.kind(TokenKind::OpenParen))
                .then(self.expr.clone().or_not())
                .then(self.parser.kind(TokenKind::CloseParen))
                .map(|((unreachable_tok, msg), close_tok)| {
                    let span = unreachable_tok.span.merge(close_tok.span);
                    Expr::new(
                        ExprKind::Unreachable {
                            msg: msg.map(Box::new),
                        },
                        span,
                    )
                }),
        ))
        .boxed()
    }

    fn ctor_fields_parser(&self) -> BoxedP<'a, Vec<PayloadField<Expr>>> {
        self.parser
            .ident_parser()
            .then(
                self.parser
                    .kind(TokenKind::Colon)
                    .ignore_then(self.expr.clone())
                    .or_not(),
            )
            .map(|(name, value)| {
                let span = match &value {
                    Some(value) => name.span.merge(value.span),
                    None => name.span,
                };
                PayloadField {
                    id: NodeId::next(),
                    name,
                    value,
                    span,
                }
            })
            .separated_by(self.parser.kind(TokenKind::Comma))
            .allow_trailing()
            .collect::<Vec<_>>()
            .boxed()
    }

    fn record_payload_parser(&self) -> BoxedP<'a, (Vec<PayloadField<Expr>>, Token)> {
        self.parser
            .kind(TokenKind::OpenBrace)
            .ignore_then(
                self.parser
                    .ident_parser()
                    .then(
                        self.parser
                            .kind(TokenKind::Colon)
                            .ignore_then(self.expr.clone())
                            .or_not(),
                    )
                    .map(|(name, value)| {
                        let span = match &value {
                            Some(value) => name.span.merge(value.span),
                            None => name.span,
                        };
                        PayloadField {
                            id: NodeId::next(),
                            name,
                            value,
                            span,
                        }
                    })
                    .separated_by(self.parser.kind(TokenKind::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then(self.parser.kind(TokenKind::CloseBrace))
            .boxed()
    }

    fn ctor_parser(&self, braces: BraceForms) -> BoxedP<'a, Expr> {
        self.brace_gated(
            braces,
            self.path
                .clone()
                .then_ignore(self.parser.kind(TokenKind::OpenBrace))
                .then(self.ctor_fields_parser())
                .then(self.parser.kind(TokenKind::CloseBrace))
                .map(|((ctor_path, payload), close_tok)| {
                    let span = ctor_path.span.merge(close_tok.span);
                    Expr::new(
                        ExprKind::Ctor {
                            path: Some(ctor_path),
                            payload,
                        },
                        span,
                    )
                })
                .boxed(),
        )
    }

    fn elided_ctor_parser(&self) -> BoxedP<'a, Expr> {
        self.parser
            .kind(TokenKind::Period)
            .then_ignore(self.parser.kind(TokenKind::OpenBrace))
            .then(self.ctor_fields_parser())
            .then(self.parser.kind(TokenKind::CloseBrace))
            .map(|((dot_tok, payload), close_tok)| {
                Expr::new(
                    ExprKind::Ctor {
                        path: None,
                        payload,
                    },
                    dot_tok.span.merge(close_tok.span),
                )
            })
            .boxed()
    }

    fn variant_parser(&self, braces: BraceForms) -> BoxedP<'a, Expr> {
        let record_variant_payload = self.brace_gated(
            braces,
            self.record_payload_parser()
                .map(|(fields, close_tok)| (Payload::Record(fields), close_tok.span))
                .boxed(),
        );
        let variant_payload = choice((
            self.parser
                .kind(TokenKind::OpenParen)
                .ignore_then(self.expr.clone())
                .then(self.parser.kind(TokenKind::CloseParen))
                .map(|(value, close_tok)| (Payload::Single(Box::new(value)), close_tok.span)),
            record_variant_payload,
        ))
        .boxed();

        self.parser
            .kind(TokenKind::Period)
            .then(self.ident.clone())
            .then(variant_payload.or_not())
            .map(|((dot_tok, variant), payload)| {
                let (payload, span) = match payload {
                    Some((payload, close_span)) => (payload, dot_tok.span.merge(close_span)),
                    None => (Payload::None, dot_tok.span.merge(variant.span)),
                };
                Expr::new(ExprKind::Variant { variant, payload }, span)
            })
            .boxed()
    }

    fn tuple_parser(&self) -> BoxedP<'a, Expr> {
        let tuple_or_group = self
            .parser
            .kind(TokenKind::OpenParen)
            .then(
                self.expr
                    .clone()
                    .separated_by(self.parser.kind(TokenKind::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then(self.parser.kind(TokenKind::CloseParen))
            .map(|((open_tok, mut exprs), close_tok)| {
                if exprs.len() == 1 {
                    exprs.pop().expect("checked len == 1 above")
                } else {
                    Expr::new(ExprKind::Tuple(exprs), open_tok.span.merge(close_tok.span))
                }
            })
            .boxed();

        let single_element_tuple = self
            .parser
            .kind(TokenKind::OpenParen)
            .then(self.expr.clone())
            .then_ignore(self.parser.kind(TokenKind::Comma))
            .then(self.parser.kind(TokenKind::CloseParen))
            .map(|((open_tok, value), close_tok)| {
                Expr::new(
                    ExprKind::Tuple(vec![value]),
                    open_tok.span.merge(close_tok.span),
                )
            })
            .boxed();

        choice((single_element_tuple, tuple_or_group)).boxed()
    }

    fn block_expr_parser(&self) -> BoxedP<'a, Expr> {
        self.block
            .clone()
            .map(|b: Block| {
                let span = b.span;
                Expr::new(ExprKind::Block(b), span)
            })
            .boxed()
    }

    fn if_parser(&self) -> BoxedP<'a, Expr> {
        let else_expr = self.else_expr();

        let if_let_expr = self
            .parser
            .kind(TokenKind::IfKw)
            .then_ignore(self.parser.kind(TokenKind::LetKw))
            .then(self.pattern.clone())
            .then_ignore(self.parser.kind(TokenKind::Equals))
            .then(self.expr_without_brace_forms.clone())
            .then(self.block.clone())
            .then(else_expr.clone())
            .map(|((((if_tok, pat), scrutinee), then_block), else_expr)| {
                let span = match &else_expr {
                    Some(e) => if_tok.span.merge(e.span),
                    None => if_tok.span.merge(then_block.span),
                };
                Expr::new(
                    ExprKind::IfLet {
                        pat,
                        scrutinee: Box::new(scrutinee),
                        then_block,
                        else_expr: else_expr.map(Box::new),
                    },
                    span,
                )
            })
            .boxed();

        let if_expr = self
            .parser
            .kind(TokenKind::IfKw)
            .then(self.expr_without_brace_forms.clone())
            .then(self.block.clone())
            .then(else_expr.clone())
            .map(|(((if_tok, cond), then_block), else_expr)| {
                let span = match &else_expr {
                    Some(e) => if_tok.span.merge(e.span),
                    None => if_tok.span.merge(then_block.span),
                };
                Expr::new(
                    ExprKind::If {
                        cond: Box::new(cond),
                        then_block,
                        else_expr: else_expr.map(Box::new),
                    },
                    span,
                )
            })
            .boxed();

        choice((if_let_expr, if_expr)).boxed()
    }

    fn match_parser(&self) -> BoxedP<'a, Expr> {
        let arm_body = choice((self.block_expr_parser(), self.expr.clone())).boxed();

        let match_arm = self
            .pattern
            .clone()
            .then(
                self.parser
                    .kind(TokenKind::IfKw)
                    .ignore_then(self.expr.clone())
                    .or_not(),
            )
            .then_ignore(self.parser.kind(TokenKind::FatArrow))
            .then(arm_body)
            .map(|((pat, guard), body)| {
                let span = pat.span.merge(body.span);
                Arm {
                    id: NodeId::next(),
                    pat,
                    guard: guard.map(Box::new),
                    body: Box::new(body),
                    span,
                }
            })
            .boxed();

        let parser = self.parser;
        let match_arms = custom(move |inp: &mut InputRef<'a, '_, &'a [Token], Extra<'a>>| {
            let mut arms = Vec::new();
            loop {
                match inp.peek() {
                    Some(t) if t.kind == TokenKind::CloseBrace => break,
                    None => break,
                    _ => {}
                }

                let arm = inp.parse(match_arm.clone())?;
                let comma_optional = matches!(arm.body.kind, ExprKind::Block(_));
                arms.push(arm);

                match inp.peek() {
                    Some(t) if t.kind == TokenKind::CloseBrace => break,
                    Some(t) if t.kind == TokenKind::Comma => inp.skip(),
                    _ if comma_optional => {}
                    _ => {
                        inp.parse(parser.kind(TokenKind::Comma))?;
                    }
                }
            }
            Ok(arms)
        })
        .boxed();

        self.parser
            .kind(TokenKind::MatchKw)
            .then(self.expr_without_brace_forms.clone())
            .then_ignore(self.parser.kind(TokenKind::OpenBrace))
            .then(match_arms)
            .then(self.parser.kind(TokenKind::CloseBrace))
            .map(|(((match_tok, scrutinee), arms), close_tok)| {
                let span = match_tok.span.merge(close_tok.span);
                Expr::new(
                    ExprKind::Match {
                        scrutinee: Box::new(scrutinee),
                        arms,
                    },
                    span,
                )
            })
            .boxed()
    }

    fn block_bodied_expr_parser(&self) -> BoxedP<'a, Expr> {
        choice((
            self.parser
                .kind(TokenKind::SpawnKw)
                .then(self.block.clone())
                .map(|(spawn_tok, body)| {
                    let span = spawn_tok.span.merge(body.span);
                    Expr::new(ExprKind::Spawn(body), span)
                })
                .boxed(),
            self.parser
                .kind(TokenKind::ConcurrentKw)
                .then(self.block.clone())
                .map(|(concurrent_tok, body)| {
                    let span = concurrent_tok.span.merge(body.span);
                    Expr::new(ExprKind::Concurrent(body), span)
                })
                .boxed(),
        ))
        .boxed()
    }

    fn closure_parser(&self) -> BoxedP<'a, Expr> {
        let type_p = self.type_parser();

        let closure_param = self
            .ident
            .clone()
            .then(
                self.parser
                    .kind(TokenKind::Colon)
                    .ignore_then(type_p.clone())
                    .or_not(),
            )
            .map(|(name, ty)| {
                let span = match &ty {
                    Some(ty) => name.span.merge(ty.span),
                    None => name.span,
                };
                ClosureParam {
                    id: NodeId::next(),
                    name,
                    ty,
                    span,
                }
            });

        let closure_params = choice((
            self.parser
                .kind(TokenKind::DoublePipe)
                .map(|t: Token| (Vec::new(), t.span)),
            self.parser
                .kind(TokenKind::Pipe)
                .then(
                    closure_param
                        .separated_by(self.parser.kind(TokenKind::Comma))
                        .allow_trailing()
                        .collect::<Vec<_>>(),
                )
                .then(self.parser.kind(TokenKind::Pipe))
                .map(|((open_tok, params), close_tok)| {
                    (params, open_tok.span.merge(close_tok.span))
                }),
        ));

        closure_params
            .then(
                self.parser
                    .kind(TokenKind::Arrow)
                    .ignore_then(type_p.clone())
                    .or_not(),
            )
            .then(self.expr.clone())
            .map(|(((params, params_span), ret), body)| {
                let span = params_span.merge(body.span);
                Expr::new(
                    ExprKind::Closure {
                        params,
                        ret,
                        body: Box::new(body),
                    },
                    span,
                )
            })
            .boxed()
    }

    fn atom_parser(&self, braces: BraceForms) -> BoxedP<'a, Expr> {
        let self_expr = self.parser.self_expr_parser();
        let self_ty_expr = self.parser.self_kw_parser();
        let decl_ref = self.path.clone().map(|p: Path| {
            let span = p.span;
            Expr::new(ExprKind::Path(p), span)
        });

        choice((
            self.closure_parser(),
            self.parser.literal_parser(),
            self.if_parser(),
            self.match_parser(),
            self.block_bodied_expr_parser(),
            self.builtin_expr_parser(),
            self.call_parser(),
            self.ctor_parser(braces),
            self.elided_ctor_parser(),
            self.variant_parser(braces),
            self_expr,
            self_ty_expr,
            decl_ref,
            self.tuple_parser(),
            self.block_expr_parser(),
        ))
        .boxed()
    }

    fn postfix_parser(&self, braces: BraceForms, atom: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        let access_record = self.brace_gated(
            braces,
            self.record_payload_parser()
                .map(|(fields, close_tok)| (AccessArgs::Record(fields), close_tok.span))
                .boxed(),
        );

        let tuple_index = self.parser.kind(TokenKind::IntLiteral).map(|t: Token| {
            let text = t.text();
            Ident {
                text: Interner::intern(&strip_digit_separators(&text)),
                span: t.span,
            }
        });

        let access_single = self
            .parser
            .kind(TokenKind::Period)
            .ignore_then(choice((self.ident.clone(), tuple_index)))
            .then(
                choice((
                    self.parser
                        .kind(TokenKind::OpenParen)
                        .ignore_then(
                            self.expr
                                .clone()
                                .separated_by(self.parser.kind(TokenKind::Comma))
                                .allow_trailing()
                                .collect::<Vec<_>>(),
                        )
                        .then(self.parser.kind(TokenKind::CloseParen))
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

        let tuple_index_pair = self.parser.kind(TokenKind::FloatLiteral).map(|t: Token| {
            let text = t.text();
            let dot = text
                .find('.')
                .expect("a `FloatLiteral` token's text always contains a '.'");
            let begin = t.span.get_begin();
            let first = Ident {
                text: Interner::intern(&strip_digit_separators(&text[..dot])),
                span: SrcSpan::new(begin, begin + dot),
            };
            let second = Ident {
                text: Interner::intern(&strip_digit_separators(&text[dot + 1..])),
                span: SrcSpan::new(begin + dot + 1, t.span.get_end()),
            };
            (first, second)
        });

        let access_pair = self
            .parser
            .kind(TokenKind::Period)
            .ignore_then(tuple_index_pair)
            .map(|(first, second)| {
                let span = second.span;
                (Postfix::TupleFieldPair(first, second), span)
            });

        let access_op = choice((access_pair, access_single));

        let index_op = self
            .parser
            .kind(TokenKind::OpenBracket)
            .ignore_then(self.expr.clone())
            .then(self.parser.kind(TokenKind::CloseBracket))
            .map(|(index, close_tok)| (Postfix::Index(index), close_tok.span));

        let try_op = self
            .parser
            .kind(TokenKind::Try)
            .map(|t: Token| (Postfix::Try, t.span));

        let postfix_op = choice((access_op, index_op, try_op));

        atom.foldl(postfix_op.repeated(), |receiver, (op, op_span)| {
            let span = receiver.span.merge(op_span);
            match op {
                Postfix::Access(member, args) => Expr::new(
                    ExprKind::Access {
                        base: Box::new(receiver),
                        member,
                        args,
                    },
                    span,
                ),
                Postfix::TupleFieldPair(first, second) => {
                    let inner_span = receiver.span.merge(first.span);
                    let inner = Expr::new(
                        ExprKind::Access {
                            base: Box::new(receiver),
                            member: first,
                            args: AccessArgs::None,
                        },
                        inner_span,
                    );
                    Expr::new(
                        ExprKind::Access {
                            base: Box::new(inner),
                            member: second,
                            args: AccessArgs::None,
                        },
                        span,
                    )
                }
                Postfix::Index(index) => Expr::new(
                    ExprKind::Index {
                        base: Box::new(receiver),
                        index: Box::new(index),
                    },
                    span,
                ),
                Postfix::Try => Expr::new(ExprKind::Try(Box::new(receiver)), span),
            }
        })
        .boxed()
    }

    fn unary_parser(&self, postfix: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        let prefix_op = choice((
            self.parser
                .kind(TokenKind::Minus)
                .map(|t: Token| (Prefix::Unary(UnaryOp::Neg), t.span)),
            self.parser
                .kind(TokenKind::Bang)
                .map(|t: Token| (Prefix::Unary(UnaryOp::Not), t.span)),
            self.parser
                .kind(TokenKind::Star)
                .map(|t: Token| (Prefix::Unary(UnaryOp::Deref), t.span)),
            self.parser
                .kind(TokenKind::Amp)
                .then(self.parser.kind(TokenKind::MutKw).or_not())
                .map(|(amp_tok, mut_tok)| {
                    let mutability = if mut_tok.is_some() {
                        Mutability::Mutable
                    } else {
                        Mutability::Immutable
                    };
                    (Prefix::Borrow(mutability), amp_tok.span)
                }),
        ));

        prefix_op
            .repeated()
            .foldr(postfix, |(op, op_span), operand| {
                let span = op_span.merge(operand.span);
                match op {
                    Prefix::Unary(op) => Expr::new(
                        ExprKind::Unary {
                            op,
                            operand: Box::new(operand),
                        },
                        span,
                    ),
                    Prefix::Borrow(mutability) => Expr::new(
                        ExprKind::Borrow {
                            mutability,
                            operand: Box::new(operand),
                        },
                        span,
                    ),
                }
            })
            .boxed()
    }

    fn new_parser(&self, unary: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        let new_array = self
            .parser
            .kind(TokenKind::NewKw)
            .then_ignore(self.parser.kind(TokenKind::OpenBracket))
            .then(self.expr.clone())
            .then_ignore(self.parser.kind(TokenKind::Semicolon))
            .then(self.expr.clone())
            .then(self.parser.kind(TokenKind::CloseBracket))
            .map(|(((new_tok, elem), count), close_tok)| {
                Expr::new(
                    ExprKind::NewArray {
                        elem: Box::new(elem),
                        count: Box::new(count),
                    },
                    new_tok.span.merge(close_tok.span),
                )
            })
            .boxed();

        let new_value = self
            .parser
            .kind(TokenKind::NewKw)
            .then(unary.clone())
            .map(|(new_tok, operand)| {
                let span = new_tok.span.merge(operand.span);
                Expr::new(ExprKind::New(Box::new(operand)), span)
            })
            .boxed();

        choice((new_array, new_value)).boxed()
    }

    fn cast_parser(&self, unary_or_new: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        unary_or_new
            .foldl(
                self.parser
                    .kind(TokenKind::AsKw)
                    .ignore_then(self.type_parser())
                    .repeated(),
                |operand, ty| {
                    let span = operand.span.merge(ty.span);
                    Expr::new(
                        ExprKind::Cast {
                            expr: Box::new(operand),
                            ty,
                        },
                        span,
                    )
                },
            )
            .boxed()
    }

    fn logical_or_parser(&self, cast: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        let bin_op =
            |k: TokenKind, op: BinaryOp| self.parser.kind(k).map(move |t: Token| (op, t.span));

        let mul_op = choice((
            bin_op(TokenKind::Star, BinaryOp::Mul),
            bin_op(TokenKind::Slash, BinaryOp::Div),
            bin_op(TokenKind::Percent, BinaryOp::Rem),
        ));
        let product = cast
            .clone()
            .foldl(mul_op.then(cast.clone()).repeated(), Expr::binary)
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
            bin_op(TokenKind::OpenAngle, BinaryOp::Lt),
            bin_op(TokenKind::CloseAngle, BinaryOp::Gt),
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
        logical_and
            .clone()
            .foldl(or_op.then(logical_and.clone()).repeated(), Expr::binary)
            .boxed()
    }

    fn range_parser(&self, logical_or: BoxedP<'a, Expr>) -> BoxedP<'a, Expr> {
        let range_op = choice((
            self.parser
                .kind(TokenKind::InclRange)
                .map(|t: Token| (true, t.span)),
            self.parser
                .kind(TokenKind::ExclRange)
                .map(|t: Token| (false, t.span)),
        ));

        let range_without_lo =
            range_op
                .clone()
                .then(logical_or.clone().or_not())
                .map(|((inclusive, op_span), hi)| {
                    let span = match &hi {
                        Some(h) => op_span.merge(h.span),
                        None => op_span,
                    };
                    desugar_range(None, hi, inclusive, span)
                });

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
                    desugar_range(Some(lo), hi, inclusive, span)
                }
            });

        choice((range_without_lo, range_with_lo)).boxed()
    }

    fn expression_parser(&self, braces: BraceForms) -> BoxedP<'a, Expr> {
        let unary = self.unary_parser(self.postfix_parser(braces, self.atom_parser(braces)));

        let unary_or_new = choice((self.new_parser(unary.clone()), unary.clone()))
            .labelled("an expression")
            .boxed();

        let range = self.range_parser(self.logical_or_parser(self.cast_parser(unary_or_new)));

        let assign_op = choice((
            self.parser
                .kind(TokenKind::Equals)
                .map(|t: Token| (None, t.span)),
            self.parser
                .kind(TokenKind::PlusEquals)
                .map(|t: Token| (Some(BinaryOp::Add), t.span)),
            self.parser
                .kind(TokenKind::MinusEquals)
                .map(|t: Token| (Some(BinaryOp::Sub), t.span)),
            self.parser
                .kind(TokenKind::MulEquals)
                .map(|t: Token| (Some(BinaryOp::Mul), t.span)),
            self.parser
                .kind(TokenKind::DivEquals)
                .map(|t: Token| (Some(BinaryOp::Div), t.span)),
            self.parser
                .kind(TokenKind::ModEquals)
                .map(|t: Token| (Some(BinaryOp::Rem), t.span)),
        ));

        range
            .clone()
            .then(assign_op.then(self.expr.clone()).or_not())
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
                    Expr::new(kind, span)
                }
            })
            .boxed()
    }

    fn decl_stmt(&self) -> BoxedP<'a, Stmt> {
        let type_p = self.type_parser();
        self.parser
            .kind(TokenKind::LetKw)
            .then(self.parser.kind(TokenKind::MutKw).or_not())
            .then(self.pattern.clone())
            .then(
                self.parser
                    .kind(TokenKind::Colon)
                    .ignore_then(type_p.clone())
                    .or_not(),
            )
            .then_ignore(self.parser.kind(TokenKind::Equals))
            .then(self.expr.clone())
            .then(
                self.parser
                    .kind(TokenKind::ElseKw)
                    .ignore_then(self.block.clone())
                    .or_not(),
            )
            .then(self.parser.kind(TokenKind::Semicolon))
            .map(
                |((((((let_tok, mut_tok), name), ty), value), else_block), semi_tok)| {
                    let mutability = if mut_tok.is_some() {
                        Mutability::Mutable
                    } else {
                        Mutability::Immutable
                    };
                    let span = let_tok.span.merge(semi_tok.span);
                    Stmt::new(
                        StmtKind::Let {
                            mutability,
                            pat: name,
                            ty,
                            init: value,
                            else_block,
                        },
                        span,
                    )
                },
            )
            .boxed()
    }

    fn with_stmt(&self) -> BoxedP<'a, Stmt> {
        let type_p = self.type_parser();
        let lend_decl = self
            .pattern
            .clone()
            .then(
                self.parser
                    .kind(TokenKind::Colon)
                    .ignore_then(type_p.clone())
                    .or_not(),
            )
            .then_ignore(self.parser.kind(TokenKind::Equals))
            .then(self.expr.clone())
            .map(|((pat, ty), value)| {
                let span = pat.span.merge(value.span);
                WithLend {
                    id: NodeId::next(),
                    pat,
                    ty,
                    init: value,
                    span,
                }
            })
            .boxed();

        self.parser
            .kind(TokenKind::WithKw)
            .then(
                lend_decl
                    .separated_by(self.parser.kind(TokenKind::Comma))
                    .at_least(1)
                    .collect::<Vec<_>>(),
            )
            .then(self.block.clone())
            .map(|((with_tok, lends), block)| {
                let span = with_tok.span.merge(block.span);
                Stmt::new(StmtKind::With { lends, block }, span)
            })
            .boxed()
    }

    fn expr_stmt(&self) -> BoxedP<'a, Stmt> {
        let expr = self.expr.clone();
        let semi = self.parser.kind(TokenKind::Semicolon);
        custom(move |inp: &mut InputRef<'a, '_, &'a [Token], Extra<'a>>| {
            let value = inp.parse(expr.clone())?;
            let semi_tok = if value.kind.is_block_bodied() {
                inp.parse(semi.clone().or_not())?
            } else {
                Some(inp.parse(semi.clone())?)
            };

            let span = match semi_tok {
                Some(semi_tok) => value.span.merge(semi_tok.span),
                None => value.span,
            };
            Ok(Stmt::new(
                StmtKind::Expr {
                    expr: value,
                    semi: semi_tok.is_some(),
                },
                span,
            ))
        })
        .boxed()
    }

    fn stmt_parser(&self) -> BoxedP<'a, Stmt> {
        let while_let_stmt = self
            .parser
            .kind(TokenKind::WhileKw)
            .then_ignore(self.parser.kind(TokenKind::LetKw))
            .then(self.pattern.clone())
            .then_ignore(self.parser.kind(TokenKind::Equals))
            .then(self.expr_without_brace_forms.clone())
            .then(self.block.clone())
            .map(|(((while_tok, pat), scrutinee), block)| {
                let span = while_tok.span.merge(block.span);
                Stmt::new(
                    StmtKind::WhileLet {
                        pat,
                        scrutinee,
                        block,
                    },
                    span,
                )
            })
            .boxed();

        let while_stmt = self
            .parser
            .kind(TokenKind::WhileKw)
            .then(self.expr_without_brace_forms.clone())
            .then(self.block.clone())
            .map(|((while_tok, cond), block)| {
                let span = while_tok.span.merge(block.span);
                Stmt::new(StmtKind::While { cond, block }, span)
            })
            .boxed();

        let for_stmt = self
            .parser
            .kind(TokenKind::ForKw)
            .then(self.pattern.clone())
            .then_ignore(self.parser.kind(TokenKind::InKw))
            .then(self.expr_without_brace_forms.clone())
            .then(self.block.clone())
            .map(|(((for_tok, pat), iter), block)| {
                let span = for_tok.span.merge(block.span);
                Stmt::new(StmtKind::For { pat, iter, block }, span)
            })
            .boxed();

        let break_stmt = self
            .parser
            .kind(TokenKind::BreakKw)
            .then(self.parser.kind(TokenKind::Semicolon))
            .map(|(break_tok, semi_tok)| {
                let span = break_tok.span.merge(semi_tok.span);
                Stmt::new(StmtKind::Break, span)
            });

        let continue_stmt = self
            .parser
            .kind(TokenKind::ContinueKw)
            .then(self.parser.kind(TokenKind::Semicolon))
            .map(|(continue_tok, semi_tok)| {
                let span = continue_tok.span.merge(semi_tok.span);
                Stmt::new(StmtKind::Continue, span)
            });

        let return_stmt = self
            .parser
            .kind(TokenKind::ReturnKw)
            .then(self.expr.clone().or_not())
            .then(self.parser.kind(TokenKind::Semicolon))
            .map(|((ret_tok, value), semi_tok)| {
                let span = ret_tok.span.merge(semi_tok.span);
                Stmt::new(StmtKind::Return(value), span)
            })
            .boxed();

        let defer_stmt = self
            .parser
            .kind(TokenKind::DeferKw)
            .then(self.expr.clone())
            .then(self.parser.kind(TokenKind::Semicolon))
            .map(|((defer_tok, value), semi_tok)| {
                let span = defer_tok.span.merge(semi_tok.span);
                Stmt::new(StmtKind::Defer(value), span)
            })
            .boxed();

        let decl_stmt = self.decl_stmt();
        let with_stmt = self.with_stmt();
        let expr_stmt = self.expr_stmt();

        let at_terminal_position = choice((
            self.parser.kind(TokenKind::CloseBrace).ignored(),
            self.expr
                .clone()
                .then(self.parser.kind(TokenKind::CloseBrace).ignored())
                .ignored(),
        ))
        .rewind();

        let stmt_recovery = at_terminal_position
            .not()
            .ignore_then(recover_by_skipping(STATEMENT_RECOVERY, |span| {
                Stmt::new(StmtKind::Error, span)
            }));

        choice((
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
        .boxed()
    }

    fn block_parser(&self) -> BoxedP<'a, Block> {
        let stmt = self.stmt_parser();
        self.parser
            .kind(TokenKind::OpenBrace)
            .then(stmt.repeated().collect::<Vec<_>>())
            .then(self.expr.clone().or_not())
            .then(self.parser.kind(TokenKind::CloseBrace))
            .map(|(((open_tok, mut stmts), tail), close_tok)| {
                if let Some(tail) = tail {
                    let span = tail.span;
                    stmts.push(Stmt::new(
                        StmtKind::Expr {
                            expr: tail,
                            semi: false,
                        },
                        span,
                    ));
                }
                Block::new(stmts, open_tok.span.merge(close_tok.span))
            })
            .boxed()
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
    fn parses_deref_expr() {
        let expr = parse_expr("*p");
        match &expr.kind {
            ExprKind::Unary {
                op: UnaryOp::Deref,
                operand,
            } => {
                assert!(matches!(operand.kind, ExprKind::Path(_)));
            }
            other => panic!("expected a deref expr, got {other:?}"),
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

    // -----------------------------------------------------------------
    // `new`
    // -----------------------------------------------------------------

    #[test]
    fn parses_new_expr() {
        let expr = parse_expr("new 1");
        match &expr.kind {
            ExprKind::New(operand) => {
                assert!(matches!(
                    operand.kind,
                    ExprKind::Literal(Literal::Int { .. })
                ));
            }
            other => panic!("expected a new expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_new_array_expr() {
        let expr = parse_expr("new [0; n]");
        match &expr.kind {
            ExprKind::NewArray { elem, count } => {
                assert!(matches!(elem.kind, ExprKind::Literal(Literal::Int { .. })));
                assert!(matches!(count.kind, ExprKind::Path(_)));
            }
            other => panic!("expected a new array expr, got {other:?}"),
        }
    }

    /// `new` binds looser than a call: `new f(x)` allocates the result of `f(x)`, not the
    /// result of allocating `f` and then calling it.
    #[test]
    fn new_binds_looser_than_a_call() {
        let expr = parse_expr("new f(x)");
        match &expr.kind {
            ExprKind::New(operand) => {
                assert!(matches!(operand.kind, ExprKind::Call { .. }));
            }
            other => panic!("expected a new expr wrapping a call, got {other:?}"),
        }
    }

    /// `new` binds tighter than any binary operator: `new x + 1` is `(new x) + 1`.
    #[test]
    fn new_binds_tighter_than_a_binary_operator() {
        let expr = parse_expr("new x + 1");
        match &expr.kind {
            ExprKind::Binary { op, lhs, .. } => {
                assert!(matches!(op, BinaryOp::Add));
                assert!(matches!(lhs.kind, ExprKind::New(_)));
            }
            other => panic!("expected a binary expr with a new lhs, got {other:?}"),
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
    fn parses_tuple_index_access() {
        let expr = parse_expr("t.0");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert!(matches!(base.kind, ExprKind::Path(_)));
                assert_eq!(Interner::resolve(member.text), "0");
                assert!(matches!(args, AccessArgs::None));
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    /// Digit separators in a tuple index are stripped the same way a numeric literal's digits
    /// are: `t.1_0` names field 10, like `t.10` does.
    #[test]
    fn parses_tuple_index_with_digit_separators() {
        let expr = parse_expr("t.1_0");
        match &expr.kind {
            ExprKind::Access { member, .. } => {
                assert_eq!(Interner::resolve(member.text), "10");
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_chained_tuple_index_access() {
        let expr = parse_expr("t.0.1");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert_eq!(Interner::resolve(member.text), "1");
                assert!(matches!(args, AccessArgs::None));
                match &base.kind {
                    ExprKind::Access { base, member, args } => {
                        assert!(matches!(base.kind, ExprKind::Path(_)));
                        assert_eq!(Interner::resolve(member.text), "0");
                        assert!(matches!(args, AccessArgs::None));
                    }
                    other => panic!("expected an access expr, got {other:?}"),
                }
            }
            other => panic!("expected an access expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_triple_chained_tuple_index_access() {
        let expr = parse_expr("t.0.1.2");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert_eq!(Interner::resolve(member.text), "2");
                assert!(matches!(args, AccessArgs::None));
                match &base.kind {
                    ExprKind::Access { base, member, args } => {
                        assert_eq!(Interner::resolve(member.text), "1");
                        assert!(matches!(args, AccessArgs::None));
                        match &base.kind {
                            ExprKind::Access { base, member, args } => {
                                assert!(matches!(base.kind, ExprKind::Path(_)));
                                assert_eq!(Interner::resolve(member.text), "0");
                                assert!(matches!(args, AccessArgs::None));
                            }
                            other => panic!("expected an access expr, got {other:?}"),
                        }
                    }
                    other => panic!("expected an access expr, got {other:?}"),
                }
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

    #[test]
    fn parses_self_kw_in_expression_position() {
        let expr = parse_expr("Self.none");
        match &expr.kind {
            ExprKind::Access { base, member, args } => {
                assert!(matches!(base.kind, ExprKind::SelfKw));
                assert_eq!(Interner::resolve(member.text), "none");
                assert!(matches!(args, AccessArgs::None));
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
    fn parses_cast_expr() {
        let expr = parse_expr("x as i64");
        match &expr.kind {
            ExprKind::Cast { expr, ty } => {
                assert!(matches!(expr.kind, ExprKind::Path(_)));
                assert!(matches!(ty.kind, crate::ast::TyKind::Path { .. }));
            }
            other => panic!("expected a cast expr, got {other:?}"),
        }
    }

    /// `as` binds looser than unary prefix operators: `-x as i64` is `(-x) as i64`.
    #[test]
    fn cast_binds_looser_than_unary_prefix() {
        let expr = parse_expr("-x as i64");
        match &expr.kind {
            ExprKind::Cast { expr, .. } => {
                assert!(matches!(
                    expr.kind,
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        ..
                    }
                ));
            }
            other => panic!("expected a cast expr, got {other:?}"),
        }
    }

    /// `as` binds tighter than every binary operator: `x as i64 + 1` is `(x as i64) + 1`, not
    /// `x as (i64 + 1)` (which isn't even a legal type).
    #[test]
    fn cast_binds_tighter_than_binary_operators() {
        let expr = parse_expr("x as i64 + 1");
        match &expr.kind {
            ExprKind::Binary {
                op: BinaryOp::Add,
                lhs,
                ..
            } => {
                assert!(matches!(lhs.kind, ExprKind::Cast { .. }));
            }
            other => panic!("expected a binary expr, got {other:?}"),
        }
    }

    /// A chain of casts is left-associative: `x as i32 as i64` casts `x` to `i32`, then that
    /// result to `i64`.
    #[test]
    fn chained_casts_are_left_associative() {
        let expr = parse_expr("x as i32 as i64");
        match &expr.kind {
            ExprKind::Cast { expr, ty } => {
                assert_eq!(base_ty_name(ty), "i64");
                match &expr.kind {
                    ExprKind::Cast { expr, ty } => {
                        assert_eq!(base_ty_name(ty), "i32");
                        assert!(matches!(expr.kind, ExprKind::Path(_)));
                    }
                    other => panic!("expected a nested cast expr, got {other:?}"),
                }
            }
            other => panic!("expected a cast expr, got {other:?}"),
        }
    }

    fn base_ty_name(ty: &crate::ast::Ty) -> &'static str {
        match &ty.kind {
            crate::ast::TyKind::Path { path, .. } => Interner::resolve(path.segments[0].text),
            other => panic!("expected a base type, got {other:?}"),
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
    fn parses_ctor_field_shorthand() {
        // `Vector2D { x, y }` is shorthand for `Vector2D { x: x, y: y }`.
        let expr = parse_expr("Vector2D { x, y: 2.0 }");
        match &expr.kind {
            ExprKind::Ctor { payload, .. } => {
                assert_eq!(payload.len(), 2);
                assert_eq!(Interner::resolve(payload[0].name.text), "x");
                assert!(payload[0].value.is_none());
                assert_eq!(Interner::resolve(payload[1].name.text), "y");
                assert!(payload[1].value.is_some());
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

    /// `(expr,)` — with its trailing comma — is a one-element tuple. The comma is what
    /// distinguishes it from the grouped expression `(expr)`, so a one-element tuple is
    /// writable in the same way a one-element tuple pattern or type is.
    #[test]
    fn parses_one_element_tuple_expr_with_trailing_comma() {
        let expr = parse_expr("(x,)");
        match &expr.kind {
            ExprKind::Tuple(elems) => assert_eq!(elems.len(), 1),
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

    /// A range desugars to `std::range::Range { left, right, inclusive }`; this pulls one
    /// named field's value back out of that `Ctor` so the range tests can check it.
    fn range_field<'a>(expr: &'a Expr, name: &str) -> &'a Expr {
        match &expr.kind {
            ExprKind::Ctor { path, payload } => {
                let path = path.as_ref().expect("a range's `Range` is never elided");
                assert_eq!(
                    path.segments
                        .iter()
                        .map(|s| Interner::resolve(s.text))
                        .collect::<Vec<_>>(),
                    vec!["std", "range", "Range"]
                );
                payload
                    .iter()
                    .find(|f| Interner::resolve(f.name.text) == name)
                    .unwrap_or_else(|| panic!("expected a `{name}` field"))
                    .value
                    .as_ref()
                    .expect("range fields always have a value")
            }
            other => panic!("expected a range expr desugared to a ctor, got {other:?}"),
        }
    }

    /// Asserts that `field` is `.some(_)` (`Some`) or bare `.none` (`None`).
    fn assert_range_bound(field: &Expr, expected: Option<()>) {
        match &field.kind {
            ExprKind::Variant { variant, payload } => {
                let name = Interner::resolve(variant.text);
                match expected {
                    Some(()) => {
                        assert_eq!(name, "some");
                        assert!(matches!(payload, Payload::Single(_)));
                    }
                    None => {
                        assert_eq!(name, "none");
                        assert!(matches!(payload, Payload::None));
                    }
                }
            }
            other => panic!("expected a range bound variant, got {other:?}"),
        }
    }

    #[test]
    fn parses_exclusive_range_expr() {
        let expr = parse_expr("0..5");
        assert_range_bound(range_field(&expr, "left"), Some(()));
        assert_range_bound(range_field(&expr, "right"), Some(()));
        assert!(matches!(
            range_field(&expr, "inclusive").kind,
            ExprKind::Literal(Literal::Bool(false))
        ));
    }

    #[test]
    fn parses_inclusive_range_expr() {
        let expr = parse_expr("0..=5");
        assert!(matches!(
            range_field(&expr, "inclusive").kind,
            ExprKind::Literal(Literal::Bool(true))
        ));
    }

    #[test]
    fn parses_range_without_lo() {
        let expr = parse_expr("..5");
        assert_range_bound(range_field(&expr, "left"), None);
        assert_range_bound(range_field(&expr, "right"), Some(()));
    }

    #[test]
    fn parses_range_without_hi() {
        let expr = parse_expr("0..");
        assert_range_bound(range_field(&expr, "left"), Some(()));
        assert_range_bound(range_field(&expr, "right"), None);
    }

    #[test]
    fn parses_full_range() {
        let expr = parse_expr("..");
        assert_range_bound(range_field(&expr, "left"), None);
        assert_range_bound(range_field(&expr, "right"), None);
    }

    #[test]
    fn parses_range_with_arithmetic_bounds() {
        // `a..b+1` should be `a..(b+1)`, since range binds looser than `+`.
        let expr = parse_expr("a..b+1");
        let hi = range_field(&expr, "right");
        let ExprKind::Variant {
            payload: Payload::Single(hi),
            ..
        } = &hi.kind
        else {
            panic!("expected `right` to be `.some(_)`, got {hi:?}");
        };
        assert!(matches!(
            hi.kind,
            ExprKind::Binary {
                op: BinaryOp::Add,
                ..
            }
        ));
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

    /// A `pat if cond => body` arm records its guard separately from the pattern; an arm with no
    /// `if` leaves it `None`.
    #[test]
    fn parses_match_arm_with_guard() {
        let expr = parse_expr("match n { x if x > 0 => 1, _ => 0 }");
        match &expr.kind {
            ExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[0].pat.kind, PatKind::Binding(_)));
                assert!(arms[0].guard.is_some());
                assert!(arms[1].guard.is_none());
            }
            other => panic!("expected a match expr, got {other:?}"),
        }
    }

    /// An arm whose body is a bare `{ .. }` block needs no comma before the next arm, since the
    /// closing brace already marks where the arm ends.
    #[test]
    fn block_bodied_match_arms_do_not_require_commas() {
        let expr = parse_expr("match s { .rectangle => { return 1; } .circle => { return 2; } }");
        match &expr.kind {
            ExprKind::Match { arms, .. } => {
                assert_eq!(arms.len(), 2);
                assert!(matches!(arms[0].body.kind, ExprKind::Block(_)));
                assert!(matches!(arms[1].body.kind, ExprKind::Block(_)));
            }
            other => panic!("expected a match expr, got {other:?}"),
        }
    }

    /// A comma between block-bodied arms is still accepted; it's optional, not forbidden.
    #[test]
    fn block_bodied_match_arms_still_allow_commas() {
        let expr = parse_expr("match s { .rectangle => { return 1; }, .circle => { return 2; } }");
        match &expr.kind {
            ExprKind::Match { arms, .. } => assert_eq!(arms.len(), 2),
            other => panic!("expected a match expr, got {other:?}"),
        }
    }

    /// An arm whose body is a plain expression, with no enclosing `{ .. }`, still requires a
    /// comma before the next arm: there would otherwise be no way to tell where the expression
    /// ends and the next arm's pattern begins.
    #[test]
    fn expr_bodied_match_arms_require_commas() {
        let (tokens, _) = lex_src("match s { .a => 1 .b => 2 }");
        let parser = Parser::new();
        let (output, errors) = parser.expr_parser().parse(&tokens[..]).into_output_errors();
        assert!(
            !errors.is_empty(),
            "expected a parse error for a missing comma, got {output:?}"
        );
    }

    /// A trailing comma after the last arm is optional regardless of that arm's body.
    #[test]
    fn parses_match_with_trailing_comma_after_expr_arm() {
        let expr = parse_expr("match s { .a => 1, .b => 2, }");
        match &expr.kind {
            ExprKind::Match { arms, .. } => assert_eq!(arms.len(), 2),
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
                assert!(matches!(rhs.kind, ExprKind::Ctor { .. }));
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

    #[test]
    fn parses_assert_with_no_message() {
        let expr = parse_expr("assert(x)");
        match &expr.kind {
            ExprKind::Assert { cond, msg } => {
                assert!(matches!(cond.kind, ExprKind::Path(_)));
                assert!(msg.is_none());
            }
            other => panic!("expected an assert expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_assert_with_a_message() {
        let expr = parse_expr(r#"assert(x, "x must be set")"#);
        match &expr.kind {
            ExprKind::Assert { cond, msg } => {
                assert!(matches!(cond.kind, ExprKind::Path(_)));
                assert!(matches!(
                    msg.as_deref(),
                    Some(Expr {
                        kind: ExprKind::Literal(_),
                        ..
                    })
                ));
            }
            other => panic!("expected an assert expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_panic_with_no_message() {
        let expr = parse_expr("panic()");
        assert!(matches!(expr.kind, ExprKind::Panic { msg: None }));
    }

    #[test]
    fn parses_panic_with_a_message() {
        let expr = parse_expr(r#"panic("oh no")"#);
        match &expr.kind {
            ExprKind::Panic { msg } => assert!(msg.is_some()),
            other => panic!("expected a panic expr, got {other:?}"),
        }
    }

    #[test]
    fn parses_unreachable_with_no_message() {
        let expr = parse_expr("unreachable()");
        assert!(matches!(expr.kind, ExprKind::Unreachable { msg: None }));
    }

    #[test]
    fn parses_unreachable_with_a_message() {
        let expr = parse_expr(r#"unreachable("should never happen")"#);
        match &expr.kind {
            ExprKind::Unreachable { msg } => assert!(msg.is_some()),
            other => panic!("expected an unreachable expr, got {other:?}"),
        }
    }
}
