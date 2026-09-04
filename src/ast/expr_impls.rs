use super::*;
use crate::ast::interner::Interner;
use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::token::Token;

fn escape_char(c: char) -> char {
    match c {
        '\'' => '\'',
        '"' => '"',
        'n' => '\n',
        't' => '\t',
        'r' => '\r',
        '\\' => '\\',
        '0' => '\0',
        other => other,
    }
}

fn escape(chars: &[char]) -> String {
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '\\' && i + 1 < chars.len() {
            out.push(escape_char(chars[i + 1]));
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out
}

fn split_suffix(text: &str) -> (&str, Option<&str>) {
    let bytes = text.as_bytes();
    for i in 0..bytes.len() {
        if bytes[i] == b'_' && bytes.get(i + 1).is_some_and(u8::is_ascii_alphabetic) {
            return (&text[..i], Some(&text[i + 1..]));
        }
    }
    (text, None)
}

impl Expr {
    pub fn new(kind: ExprKind, span: SrcSpan) -> Self {
        Expr {
            id: NodeId::next(),
            kind,
            span,
        }
    }

    pub fn int(tok: Token) -> Expr {
        let text = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        let (value, suffix) = split_suffix(&text);

        Expr {
            id: NodeId::next(),
            kind: ExprKind::Literal(Literal::Int {
                value: Interner::intern(value),
                suffix: suffix.map(Interner::intern),
            }),
            span: tok.span,
        }
    }

    pub fn float(tok: Token) -> Expr {
        let text = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        let (value, suffix) = split_suffix(&text);

        Expr {
            id: NodeId::next(),
            kind: ExprKind::Literal(Literal::Float {
                value: Interner::intern(value),
                suffix: suffix.map(Interner::intern),
            }),
            span: tok.span,
        }
    }

    pub fn string(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        Expr {
            id: NodeId::next(),
            kind: ExprKind::Literal(Literal::Str(Interner::intern(&escape(&inner)))),
            span: tok.span,
        }
    }

    pub fn char(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        let ch = escape(&inner).chars().next().unwrap_or('\0');
        Expr {
            id: NodeId::next(),
            kind: ExprKind::Literal(Literal::Char(ch)),
            span: tok.span,
        }
    }

    pub fn binary(lhs: Expr, ((op, _op_span), rhs): ((BinaryOp, SrcSpan), Expr)) -> Self {
        let span = lhs.span.merge(rhs.span);
        Expr::new(
            ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            span,
        )
    }
}
