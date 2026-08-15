use super::*;
use crate::ast::interner::Interner;
use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::token::Token;

/// Maps the character following a `\` to the value it escapes. Returns unrecognized characters
/// unchanged, so an unknown escape does not stop lexing.
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

/// The lexer only checks that a string or char literal's escapes are valid. It does not convert
/// them to their real values, since it works with spans, not owned strings. Later stages need
/// the actual value, so this function does that conversion.
fn unescape(chars: &[char]) -> String {
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

/// Splits a lexed number's text into its value and, if present, its type suffix (the `i64` in
/// `42_i64`, without the leading `_`).
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

    /// Builds an integer literal expression from its token, e.g. `42` or the suffixed `42_i64`.
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

    /// Builds a float literal expression from its token, e.g. `3.14` or the suffixed
    /// `3.14_f32`. See [`Expr::int`] for the suffix handling; the two are shared through
    /// [`split_suffix`] below.
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

    /// Builds a string literal expression from a `"..."` token, unescaping its contents.
    pub fn string(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        Expr {
            id: NodeId::next(),
            kind: ExprKind::Literal(Literal::Str(Interner::intern(&unescape(&inner)))),
            span: tok.span,
        }
    }

    /// Builds a char literal expression from a `'...'` token, unescaping its contents.
    pub fn char(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        let ch = unescape(&inner).chars().next().unwrap_or('\0');
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
