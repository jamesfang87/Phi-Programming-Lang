use chumsky::error::{Rich, RichPattern, RichReason};

use crate::diagnostics::Diagnostic;
use crate::diagnostics::codes;
use crate::diagnostics::wording::{self, Expected};
use crate::driver::source::SrcSpan;
use crate::lexer::token::{Token, TokenKind};
use crate::session::Session;

pub fn unclosed_delimiter(open: Token, close: TokenKind) -> Diagnostic {
    Diagnostic::error(format!("unclosed `{open}`"), open.span)
        .with_code(codes::UNCLOSED_DELIMITER)
        .with_label(format!("this `{open}` has no matching `{close}`"))
        .with_help(format!("add a `{close}` to close it"))
}

pub fn mismatched_delimiter(open: Token, close: TokenKind, found: Token) -> Diagnostic {
    Diagnostic::error(
        format!("mismatched closing delimiter: expected `{close}`, found `{found}`"),
        found.span,
    )
    .with_code(codes::MISMATCHED_DELIMITER)
    .with_label(format!("expected `{close}` here"))
    .with_secondary(open.span, format!("this `{open}` is still open"))
}

pub fn unopened_delimiter(found: Token) -> Diagnostic {
    Diagnostic::error(format!("unmatched `{found}`"), found.span)
        .with_code(codes::UNOPENED_DELIMITER)
        .with_label(format!(
            "there is no `{}` for this to close",
            opener_of(found.kind)
        ))
}

fn opener_of(kind: TokenKind) -> TokenKind {
    match kind {
        TokenKind::CloseParen => TokenKind::OpenParen,
        TokenKind::CloseBracket => TokenKind::OpenBracket,
        TokenKind::CloseBrace => TokenKind::OpenBrace,
        other => unreachable!("{other} is not a closing delimiter"),
    }
}

pub fn report_duplicate_module(session: &Session, span: SrcSpan) {
    session.emit(
        Diagnostic::error("a file can only declare one module", span)
            .with_code(codes::DUPLICATE_MODULE)
            .with_label("second `module` declaration")
            .with_help("every item in a file belongs to the module its first header names"),
    );
}

pub fn report_parse(
    session: &Session,
    errors: &[Rich<Token>],
    tokens: &[Token],
    file_offset: usize,
    delimiter_diagnostics: Vec<Diagnostic>,
) {
    if delimiter_diagnostics.is_empty() {
        report_errors(session, errors, tokens, file_offset);
    } else {
        delimiter_diagnostics
            .into_iter()
            .for_each(|diag| session.emit(diag));
    }
}

fn report_errors(session: &Session, errors: &[Rich<Token>], tokens: &[Token], file_offset: usize) {
    let mut reported: Vec<SrcSpan> = Vec::new();

    for err in errors {
        let (span, preceding) = source_span(err, tokens, file_offset);

        if reported.contains(&span) {
            continue;
        }
        reported.push(span);

        session.emit(diagnostic_for(session, err, span, preceding));
    }
}

fn diagnostic_for(
    session: &Session,
    err: &Rich<Token>,
    span: SrcSpan,
    preceding: Option<Token>,
) -> Diagnostic {
    if let RichReason::Custom(message) = err.reason() {
        return Diagnostic::error(message.clone(), span)
            .with_code(codes::PARSE_ERROR)
            .with_label(message.clone());
    }

    let found = err.found().copied();
    let expected = expected_set(err);
    let message = wording::message(&expected, found.as_ref().map(|token| token.kind));
    let diagnostic = Diagnostic::error(message, span).with_label(wording::label(&expected));

    match wording::help_for(session, &expected, found, preceding) {
        Some(help) => diagnostic.with_help(help),
        None => diagnostic,
    }
}

fn expected_set(err: &Rich<Token>) -> Vec<Expected> {
    let mut expected: Vec<Expected> = Vec::new();

    for pattern in err.expected() {
        let alternative = match pattern {
            RichPattern::Token(token) => Expected::of_kind(token.kind),
            RichPattern::Label(label) => Expected::decode(label),
            RichPattern::EndOfInput => Expected::of_kind(TokenKind::Eof),
            _ => continue,
        };
        if !expected.contains(&alternative) {
            expected.push(alternative);
        }
    }

    expected
}

fn source_span(
    err: &Rich<Token>,
    tokens: &[Token],
    file_offset: usize,
) -> (SrcSpan, Option<Token>) {
    let range = err.span().into_range();

    let Some(first) = tokens.get(range.start) else {
        let span = match tokens.last() {
            Some(last) => last.span,
            None => SrcSpan::new(file_offset, file_offset),
        };
        return (span, None);
    };

    let preceding = range
        .start
        .checked_sub(1)
        .and_then(|i| tokens.get(i))
        .copied();

    let last = tokens[..range.end.min(tokens.len())]
        .last()
        .unwrap_or(first);
    (first.span.merge(last.span), preceding)
}
