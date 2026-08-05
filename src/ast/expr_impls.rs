//! Constructors that build [`Expr`] literal and binary nodes directly from lexer tokens.

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

impl Expr {
    pub fn new(kind: ExprKind, span: SrcSpan) -> Self {
        Expr { kind, span }
    }

    /// Builds an integer literal expression from a value token and a suffix token.
    ///
    /// Every current call site passes the literal's own token as both arguments, since the
    /// lexer does not yet split a type suffix, such as the `i32` in `42i32`, into its own
    /// token.
    pub fn int(value_tok: Token, suffix_tok: Token) -> Expr {
        let value_text = SrcMap::text_of(value_tok.span)
            .expect("lexer token span should always resolve to a source file");
        let suffix_text = SrcMap::text_of(suffix_tok.span)
            .expect("lexer token span should always resolve to a source file");

        Expr {
            kind: ExprKind::Literal(Literal::Int {
                value: Interner::intern(&value_text),
                suffix: Interner::intern(&suffix_text),
            }),
            span: value_tok.span.merge(suffix_tok.span),
        }
    }

    /// Builds a float literal expression. See [`Expr::int`] for why it takes two tokens.
    pub fn float(value_tok: Token, suffix_tok: Token) -> Expr {
        let value_text = SrcMap::text_of(value_tok.span)
            .expect("lexer token span should always resolve to a source file");
        let suffix_text = SrcMap::text_of(suffix_tok.span)
            .expect("lexer token span should always resolve to a source file");

        Expr {
            kind: ExprKind::Literal(Literal::Float {
                value: Interner::intern(&value_text),
                suffix: Interner::intern(&suffix_text),
            }),
            span: value_tok.span.merge(suffix_tok.span),
        }
    }

    /// Builds a string literal expression from a `"..."` token, unescaping its contents.
    pub fn string(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        Expr {
            kind: ExprKind::Literal(Literal::Str(Interner::intern(&unescape(&inner)))),
            span: tok.span,
        }
    }

    /// Builds a char literal expression from a `'...'` token, unescaping its contents.
    ///
    /// Falls back to `'\0'` when the contents unescape to nothing. That happens for an empty
    /// `''` literal, which the lexer still emits as a token, with an error already recorded,
    /// so the parser can recover and keep going.
    pub fn char(tok: Token) -> Expr {
        let chars = SrcMap::text_of(tok.span)
            .expect("lexer token span should always resolve to a source file");
        // Drop the surrounding quote characters before unescaping.
        let inner: Vec<char> = chars[1..chars.len() - 1].chars().collect();
        let ch = unescape(&inner).chars().next().unwrap_or('\0');
        Expr {
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
