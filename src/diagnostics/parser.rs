use chumsky::error::{Rich, RichPattern, RichReason};

use crate::diagnostics::{DiagCtx, Diagnostic};
use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::token::{Token, TokenKind};

/// How many alternatives [`render_alternatives`] names before it truncates.
const MAX_LISTED_ALTERNATIVES: usize = 4;

/// The same limit for a set of nothing but keywords. See [`render_alternatives`].
const MAX_LISTED_KEYWORDS: usize = 8;

/// The tokens that end or separate a construct.
///
/// [`render_alternatives`] keeps these and discards the rest. After a complete expression every
/// binary, postfix, and assignment operator is also a legal continuation, so an untrimmed
/// expected set at that position is roughly 25 operators and one `;`.
const TERMINATORS: [TokenKind; 8] = [
    TokenKind::Semicolon,
    TokenKind::Comma,
    TokenKind::CloseParen,
    TokenKind::CloseBrace,
    TokenKind::CloseBracket,
    TokenKind::Arrow,
    TokenKind::FatArrow,
    TokenKind::Colon,
];

pub fn report_duplicate_module(span: SrcSpan) {
    DiagCtx::emit(
        Diagnostic::error("a file can only declare one module", span)
            .with_label("second `module` declaration")
            .with_help("every item in a file belongs to the module its first header names"),
    );
}

/// Emits one [`Diagnostic`] per distinct failure position in `errors`.
///
/// `tokens` must be the slice the grammar ran over: [`Rich::span`] holds indices into it, and
/// [`source_span`] needs the tokens to turn those into a [`SrcSpan`]. `file_offset` is used only
/// when `tokens` is empty and there is no token to take a span from.
pub fn report_errors(errors: &[Rich<Token>], tokens: &[Token], file_offset: usize) {
    let mut reported: Vec<SrcSpan> = Vec::new();

    for err in errors {
        let span = source_span(err, tokens, file_offset);

        // More than one alternative can fail at the same token, giving several `Rich` errors for
        // one mistake. The first carries the union of their expected sets.
        if reported.contains(&span) {
            continue;
        }
        reported.push(span);

        DiagCtx::emit(diagnostic_for(err, span));
    }
}

fn diagnostic_for(err: &Rich<Token>, span: SrcSpan) -> Diagnostic {
    if let RichReason::Custom(message) = err.reason() {
        return Diagnostic::error(message.clone(), span).with_label(message.clone());
    }

    let found = err.found().copied();
    let expected = expected_names(err);
    let found_text = match found {
        Some(token) => token.kind.describe(),
        None => "end of file".to_string(),
    };

    // `render_alternatives` decides what goes in the message; `help_for` reads the full set, so
    // an alternative dropped from the message can still produce a suggestion.
    let diagnostic = match render_alternatives(&expected) {
        Some(named) => Diagnostic::error(format!("expected {named}, found {found_text}"), span)
            .with_label(format!("expected {named} here")),
        None => Diagnostic::error(format!("unexpected {found_text}"), span)
            .with_label("this doesn't fit here"),
    };

    match help_for(&expected, found) {
        Some(help) => diagnostic.with_help(help),
        None => diagnostic,
    }
}

/// Names every pattern in [`Rich::expected`], deduplicated, in the order chumsky recorded them.
///
/// A [`RichPattern::Label`] holds either a [`TokenKind::describe`] string, written by
/// `Parser::kind`, or the name of a whole construct, written by the `labelled` calls in the
/// grammar; [`token_spelling`] tells the two apart. [`RichPattern::Any`] and
/// [`RichPattern::SomethingElse`] are skipped: they come from the `any()` inside
/// `Parser::recover_by_skipping` and name no syntax.
fn expected_names(err: &Rich<Token>) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();

    for pattern in err.expected() {
        let name = match pattern {
            RichPattern::Token(token) => token.kind.describe(),
            RichPattern::Label(label) => label.to_string(),
            RichPattern::Identifier(name) => format!("`{name}`"),
            RichPattern::EndOfInput => "end of file".to_string(),
            _ => continue,
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }

    names
}

/// Returns the source text `name` stands for, or `None` if `name` is a construct or a class of
/// lexeme rather than a single spelling.
///
/// [`TokenKind::describe`] wraps a fixed spelling in backticks and leaves every other name bare,
/// so the backticks are what identify a name as literal source text.
fn token_spelling(name: &str) -> Option<&str> {
    name.strip_prefix('`')?.strip_suffix('`')
}

fn is_construct_name(name: &str) -> bool {
    token_spelling(name).is_none()
}

fn is_terminator(name: &str) -> bool {
    TERMINATORS.iter().any(|kind| kind.describe() == name)
}

/// Renders `names` as the `a`, `a or b`, or `a, b, or c` that follows "expected" in the message.
///
/// A set longer than the limit first drops the alternatives that would only have continued the
/// construct, keeping the construct names and [`TERMINATORS`] entries that would have ended it.
/// A set with none of those to fall back on keeps everything and is truncated instead.
///
/// The limit is `MAX_LISTED_ALTERNATIVES`, or `MAX_LISTED_KEYWORDS` when every alternative is a
/// keyword. The keywords that can open an item, for instance, are a closed set of eight, and
/// naming all of them says what an item is; "or 4 other kinds of token" does not.
fn render_alternatives(names: &[String]) -> Option<String> {
    let terminates_construct = |name: &String| is_construct_name(name) || is_terminator(name);

    let limit = if names.iter().all(|name| is_keyword_name(name)) {
        MAX_LISTED_KEYWORDS
    } else {
        MAX_LISTED_ALTERNATIVES
    };

    let kept: Vec<&String> = if names.len() > limit && names.iter().any(terminates_construct) {
        names
            .iter()
            .filter(|name| terminates_construct(name))
            .collect()
    } else {
        names.iter().collect()
    };

    match kept.as_slice() {
        [] => None,
        [only] => Some((*only).clone()),
        [first, second] => Some(format!("{first} or {second}")),
        many if many.len() <= limit => {
            let (last, rest) = many
                .split_last()
                .expect("`many` has at least three elements");
            let rest: Vec<&str> = rest.iter().map(|name| name.as_str()).collect();
            Some(format!("{}, or {last}", rest.join(", ")))
        }
        many => {
            let shown: Vec<&str> = many[..limit].iter().map(|name| name.as_str()).collect();
            let hidden = many.len() - limit;
            Some(format!(
                "{}, or {hidden} other kinds of token",
                shown.join(", ")
            ))
        }
    }
}

fn is_keyword_name(name: &str) -> bool {
    token_spelling(name).is_some_and(spelled_as_word)
}

/// Returns the text for the diagnostic's `help` field, for the failures where "expected X, found
/// Y" does not say which edit fixes the line.
fn help_for(expected: &[String], found: Option<Token>) -> Option<String> {
    let accepts = |kind: TokenKind| expected.iter().any(|name| *name == kind.describe());
    let Some(found) = found else {
        return Some("the file ends in the middle of this construct".to_string());
    };

    if let Some(keyword) = suggested_keyword(expected, found) {
        return Some(format!("did you mean `{keyword}`?"));
    }
    if accepts(TokenKind::Identifier) && found.kind.is_word() {
        let word = found.kind;
        return Some(format!(
            "`{word}` is a keyword, so it cannot be used as a name"
        ));
    }
    if accepts(TokenKind::Semicolon) && starts_a_statement(found.kind) {
        return Some("the statement before this one is missing its `;`".to_string());
    }
    // A closing delimiter ends the list, so no separator was missing before it.
    if accepts(TokenKind::Comma) && !found.kind.closes_group() {
        return Some("the entry before this one is missing its `,`".to_string());
    }
    if accepts(TokenKind::Arrow) && (starts_a_type(found.kind) || found.kind == TokenKind::Colon) {
        return Some("a return type is written after `->`, as in `fun f() -> i32`".to_string());
    }
    if accepts(TokenKind::Colon) && starts_a_type(found.kind) {
        return Some("a name and its type are separated by `:`".to_string());
    }

    None
}

/// Returns the keyword in `expected` closest to the identifier `found`, if one is close enough
/// under [`is_probable_typo_of`].
///
/// Candidates are drawn from `expected` rather than from every keyword in the language, so a
/// suggestion always parses at this position. Returns `None` when `expected` accepts an
/// identifier: the parse then failed for some other reason and the name itself is not at fault.
fn suggested_keyword(expected: &[String], found: Token) -> Option<String> {
    let accepts_a_name = expected
        .iter()
        .any(|name| *name == TokenKind::Identifier.describe());
    if found.kind != TokenKind::Identifier || accepts_a_name {
        return None;
    }
    let written = SrcMap::text_of(found.span)?;

    expected
        .iter()
        .filter_map(|name| token_spelling(name))
        .filter(|spelling| spelled_as_word(spelling) && is_probable_typo_of(&written, spelling))
        .min_by_key(|spelling| edit_distance(&written, spelling))
        .map(str::to_string)
}

fn spelled_as_word(text: &str) -> bool {
    text.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Whether `written` is within one edit of `keyword`, or two once `written` reaches four
/// characters, or is a prefix of it at least two characters long.
///
/// The prefix case covers a keyword spelled the way another language spells it, such as `pub`
/// for `public`. The length-dependent threshold keeps two edits from turning a three-letter
/// identifier into an unrelated keyword.
fn is_probable_typo_of(written: &str, keyword: &str) -> bool {
    if written.len() >= 2 && keyword.starts_with(written) {
        return true;
    }
    let allowed = if written.len() >= 4 { 2 } else { 1 };
    edit_distance(written, keyword) <= allowed
}

/// The Damerau-Levenshtein distance between `a` and `b`: the number of single-character
/// insertions, deletions, substitutions, and transpositions of adjacent characters that turn one
/// into the other.
///
/// Transpositions count as one edit rather than the two that plain Levenshtein charges, so
/// `strcut` is one edit from `struct` and gets suggested for it.
fn edit_distance(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();

    let mut table = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in table.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in table[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let substitute = table[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]);
            let mut best = substitute.min(table[i - 1][j] + 1).min(table[i][j - 1] + 1);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                best = best.min(table[i - 2][j - 2] + 1);
            }
            table[i][j] = best;
        }
    }

    table[a.len()][b.len()]
}

fn starts_a_statement(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::LetKw
            | TokenKind::ReturnKw
            | TokenKind::WhileKw
            | TokenKind::ForKw
            | TokenKind::IfKw
            | TokenKind::MatchKw
            | TokenKind::BreakKw
            | TokenKind::ContinueKw
            | TokenKind::DeferKw
            | TokenKind::WithKw
    )
}

fn starts_a_type(kind: TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::Identifier
            | TokenKind::UpperSelfKw
            | TokenKind::I8
            | TokenKind::I16
            | TokenKind::I32
            | TokenKind::I64
            | TokenKind::U8
            | TokenKind::U16
            | TokenKind::U32
            | TokenKind::U64
            | TokenKind::Usize
            | TokenKind::F32
            | TokenKind::F64
            | TokenKind::BoolKw
            | TokenKind::Str
            | TokenKind::Char
            | TokenKind::Amp
            | TokenKind::AnyKw
            | TokenKind::DynKw
            | TokenKind::IsoKw
    )
}

/// Converts `err`'s span, which holds indices into `tokens`, to the [`SrcSpan`] those tokens
/// cover.
///
/// A failure at end of input has a span starting past the last index; it is given the last
/// token's span, since there is no later text to point at.
fn source_span(err: &Rich<Token>, tokens: &[Token], file_offset: usize) -> SrcSpan {
    let range = err.span().into_range();

    let Some(first) = tokens.get(range.start) else {
        return match tokens.last() {
            Some(last) => last.span,
            None => SrcSpan::new(file_offset, file_offset),
        };
    };

    let last = tokens[..range.end.min(tokens.len())]
        .last()
        .unwrap_or(first);
    first.span.merge(last.span)
}
