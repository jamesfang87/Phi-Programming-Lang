//! This code allows for the emission of errors relating to unmatchined `(`, `[` and `{`
//! while preserving future errors.

use crate::diagnostics::Diagnostic;
use crate::diagnostics::parser::{mismatched_delimiter, unopened_delimiter, unclosed_delimiter};
use crate::lexer::token::{Token, TokenKind};

struct OpenDelimiter {
    open: Token,
    close: TokenKind,
}

/// Returns the closing delimiter `kind` opens, or `None` if `kind` opens nothing.
fn corresponding_closing_delimiter(kind: TokenKind) -> Option<TokenKind> {
    match kind {
        TokenKind::OpenParen => Some(TokenKind::CloseParen),
        TokenKind::OpenBracket => Some(TokenKind::CloseBracket),
        TokenKind::OpenBrace => Some(TokenKind::CloseBrace),
        _ => None,
    }
}

pub fn find_unmatched_delimiter_errors(tokens: &[Token]) -> Vec<Diagnostic> {
    let mut open: Vec<OpenDelimiter> = Vec::new();

    for token in tokens {
        if let Some(close) = corresponding_closing_delimiter(token.kind) {
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
            Some(innermost) => {
                return vec![mismatched_delimiter(innermost.open, innermost.close, *token)]
            }
            None => return vec![unopened_delimiter(*token)],
        }
    }

    open.iter()
        .map(|delimiter| unclosed_delimiter(delimiter.open, delimiter.close))
        .collect()
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
        find_unmatched_delimiter_errors(&tokens)
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
