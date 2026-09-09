//! Matches up `(`, `[` and `{` over a token stream, ahead of the grammar.
//!
//! An unclosed delimiter makes the grammar run to the end of the file before failing, so its
//! error lands there rather than on the opening delimiter, and every construct in between fails
//! as well. Pairing the delimiters separately gives `Parser::run` a diagnostic that carries the
//! opening delimiter's span, and a reason to suppress the grammar's own errors for that file.
//!
//! `<` and `>` are not tracked. They are comparison operators as often as they are generic
//! argument brackets, so they do not pair up even in source that parses.

use crate::diagnostics::Diagnostic;
use crate::lexer::token::{Token, TokenKind};

/// An opening delimiter whose closing partner has not been seen yet.
struct OpenDelimiter {
    open: Token,
    close: TokenKind,
}

/// Returns the closing delimiter `kind` opens, or `None` if `kind` opens nothing.
fn closer_of(kind: TokenKind) -> Option<TokenKind> {
    match kind {
        TokenKind::OpenParen => Some(TokenKind::CloseParen),
        TokenKind::OpenBracket => Some(TokenKind::CloseBracket),
        TokenKind::OpenBrace => Some(TokenKind::CloseBrace),
        _ => None,
    }
}

/// Returns a diagnostic for each delimiter in `tokens` left without a partner, or an empty
/// vector if every one of them pairs up.
///
/// A closing delimiter that does not match the innermost open one stops the scan: after it the
/// stack no longer reflects the source, so any further pairing would be a guess. Delimiters left
/// open at the end of `tokens` are all reported, since each is an independent missing `)`, `]`
/// or `}`.
pub fn unmatched(tokens: &[Token]) -> Vec<Diagnostic> {
    let mut open: Vec<OpenDelimiter> = Vec::new();

    for token in tokens {
        if let Some(close) = closer_of(token.kind) {
            open.push(OpenDelimiter {
                open: *token,
                close,
            });
            continue;
        }
        if !token.kind.closes_group() {
            continue;
        }

        match open.last() {
            Some(innermost) if innermost.close == token.kind => {
                open.pop();
            }
            Some(innermost) => return vec![mismatch_error(innermost, *token)],
            None => return vec![unopened_error(*token)],
        }
    }

    open.iter().map(unclosed_error).collect()
}

fn unclosed_error(delimiter: &OpenDelimiter) -> Diagnostic {
    let open = delimiter.open.kind;
    let close = delimiter.close;
    Diagnostic::error(format!("unclosed `{open}`"), delimiter.open.span)
        .with_label(format!("this `{open}` has no matching `{close}`"))
        .with_help(format!("add a `{close}` to close it"))
}

fn mismatch_error(innermost: &OpenDelimiter, found: Token) -> Diagnostic {
    let open = innermost.open.kind;
    let close = innermost.close;
    Diagnostic::error(
        format!("mismatched closing delimiter: expected `{close}`, found `{found}`"),
        found.span,
    )
    .with_label(format!("expected `{close}` here"))
    .with_secondary(innermost.open.span, format!("this `{open}` is still open"))
}

fn unopened_error(found: Token) -> Diagnostic {
    Diagnostic::error(format!("unmatched `{found}`"), found.span).with_label(format!(
        "there is no `{}` for this to close",
        opener_of(found.kind)
    ))
}

/// Returns the opening delimiter `kind` closes. Panics on any other kind.
fn opener_of(kind: TokenKind) -> TokenKind {
    match kind {
        TokenKind::CloseParen => TokenKind::OpenParen,
        TokenKind::CloseBracket => TokenKind::OpenBracket,
        TokenKind::CloseBrace => TokenKind::OpenBrace,
        other => unreachable!("{other} is not a closing delimiter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::source::SrcMap;
    use crate::testing::lex_src;

    /// Returns the message of each diagnostic `src` raises, paired with the source text its
    /// primary span covers.
    fn unmatched_in(src: &str) -> Vec<(String, String)> {
        let (tokens, _) = lex_src(src);
        unmatched(&tokens)
            .into_iter()
            .map(|diag| {
                let span = diag
                    .span
                    .expect("every delimiter diagnostic carries the span of a token");
                let at =
                    SrcMap::text_of(span).expect("the span comes from a token the lexer produced");
                (diag.message, at)
            })
            .collect()
    }

    #[test]
    fn balanced_source_raises_nothing() {
        assert!(unmatched_in("fun main() { let xs = [f(1), g(2)]; }").is_empty());
    }

    #[test]
    fn caret_is_not_treated_as_a_delimiter() {
        // `<` and `>` are comparisons here, not generic brackets, and must not be paired up.
        assert!(unmatched_in("fun main() { if a < b { c > d; } }").is_empty());
    }

    #[test]
    fn an_unclosed_brace_points_at_the_brace_itself() {
        assert_eq!(
            unmatched_in("fun main() { let x = 1;"),
            [("unclosed `{`".to_string(), "{".to_string())]
        );
    }

    #[test]
    fn every_delimiter_left_open_is_reported() {
        let messages: Vec<String> = unmatched_in("fun main() { foo(")
            .into_iter()
            .map(|(message, _)| message)
            .collect();
        assert_eq!(messages, ["unclosed `{`", "unclosed `(`"]);
    }

    /// The `}` closes the function, but the `(` opened inside it is still waiting for a `)`.
    #[test]
    fn a_close_that_does_not_match_the_innermost_open_is_reported_once() {
        assert_eq!(
            unmatched_in("fun main() { foo(1; }"),
            [(
                "mismatched closing delimiter: expected `)`, found `}`".to_string(),
                "}".to_string()
            )]
        );
    }

    #[test]
    fn a_close_with_nothing_open_is_reported() {
        assert_eq!(
            unmatched_in("fun main() {} }"),
            [("unmatched `}`".to_string(), "}".to_string())]
        );
    }
}
