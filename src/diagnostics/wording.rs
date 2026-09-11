use std::borrow::Cow;

use crate::driver::source::SrcMap;
use crate::lexer::describe::{Descriptor, is_word_spelling, quoted_spelling};
use crate::lexer::token::{STATEMENT_STARTERS, Token, TokenKind};

const MAX_LISTED_ALTERNATIVES: usize = 4;

const MAX_LISTED_KEYWORDS: usize = 8;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Expected {
    Spelling(String),
    Named(String),
}

impl From<Descriptor> for Expected {
    fn from(descriptor: Descriptor) -> Self {
        match descriptor {
            Descriptor::Spelling(spelling) => Expected::Spelling(spelling.to_owned()),
            Descriptor::Named(name) => Expected::Named(name.to_owned()),
        }
    }
}

impl Expected {
    pub(crate) fn decode(name: &str) -> Self {
        match name
            .strip_prefix('`')
            .and_then(|rest| rest.strip_suffix('`'))
        {
            Some(spelling) => Expected::Spelling(spelling.to_string()),
            None => Expected::Named(name.to_string()),
        }
    }

    pub(crate) fn of_kind(kind: TokenKind) -> Self {
        Descriptor::of(kind).into()
    }

    fn spelling(&self) -> Option<&str> {
        match self {
            Expected::Spelling(spelling) => Some(spelling),
            Expected::Named(_) => None,
        }
    }

    fn is_keyword(&self) -> bool {
        self.spelling().is_some_and(is_word_spelling)
    }

    pub(crate) fn accepts(&self, kind: TokenKind) -> bool {
        let descriptor = Descriptor::of(kind);
        match (self, descriptor.spelling()) {
            (Expected::Spelling(spelling), Some(expected)) => spelling == expected,
            (Expected::Named(name), None) => name == descriptor.name(),
            _ => false,
        }
    }

    fn name(&self) -> Cow<'_, str> {
        match self {
            Expected::Spelling(spelling) => Cow::Owned(quoted_spelling(spelling)),
            Expected::Named(name) => Cow::Borrowed(name),
        }
    }
}

pub(crate) fn message(expected: &[Expected], found: Option<TokenKind>) -> String {
    let found_text = found.map_or_else(
        || "end of file".to_string(),
        |kind| Descriptor::of(kind).describe(),
    );
    match render_alternatives(expected) {
        Some(named) => format!("expected {named}, found {found_text}"),
        None => format!("unexpected {found_text}"),
    }
}

pub(crate) fn label(expected: &[Expected]) -> String {
    match render_alternatives(expected) {
        Some(named) => format!("expected {named} here"),
        None => "this doesn't fit here".to_string(),
    }
}

fn render_alternatives(expected: &[Expected]) -> Option<String> {
    let terminates_construct = |alternative: &Expected| {
        matches!(alternative, Expected::Named(_)) || {
            TERMINATORS.iter().any(|kind| alternative.accepts(*kind))
        }
    };

    let limit = if expected.iter().all(Expected::is_keyword) {
        MAX_LISTED_KEYWORDS
    } else {
        MAX_LISTED_ALTERNATIVES
    };

    let kept: Vec<&Expected> =
        if expected.len() > limit && expected.iter().any(terminates_construct) {
            expected
                .iter()
                .filter(|a| terminates_construct(a))
                .collect()
        } else {
            expected.iter().collect()
        };

    let rendered: Vec<Cow<'_, str>> = kept.iter().map(|alternative| alternative.name()).collect();
    match rendered.as_slice() {
        [] => None,
        [only] => Some(only.to_string()),
        [first, second] => Some(format!("{first} or {second}")),
        many if many.len() <= limit => {
            let (last, rest) = many
                .split_last()
                .expect("`many` has at least three elements");
            let rest: Vec<&str> = rest.iter().map(|name| name.as_ref()).collect();
            Some(format!("{}, or {last}", rest.join(", ")))
        }
        many => {
            let shown: Vec<&str> = many[..limit].iter().map(|name| name.as_ref()).collect();
            let hidden = many.len() - limit;
            Some(format!(
                "{}, or {hidden} other kinds of token",
                shown.join(", ")
            ))
        }
    }
}

pub(crate) fn help_for(
    expected: &[Expected],
    found: Option<Token>,
    preceding: Option<Token>,
) -> Option<String> {
    let accepts = |kind: TokenKind| expected.iter().any(|alternative| alternative.accepts(kind));
    let Some(found) = found else {
        return Some("the file ends in the middle of this construct".to_string());
    };

    if let Some(keyword) = suggested_keyword(expected, found) {
        return Some(format!("did you mean `{keyword}`?"));
    }
    if accepts(TokenKind::Identifier) && Descriptor::of(found.kind).is_word() {
        let word = found.kind;
        return Some(format!(
            "`{word}` is a keyword, so it cannot be used as a name"
        ));
    }
    if accepts(TokenKind::Semicolon) && starts_a_statement(found.kind) {
        return Some("the statement before this one is missing its `;`".to_string());
    }
    if accepts(TokenKind::Semicolon)
        && found.kind == TokenKind::Identifier
        && preceding.is_some_and(|prev| {
            matches!(prev.kind, TokenKind::IntLiteral | TokenKind::FloatLiteral)
        })
    {
        return Some(
            "a number cannot be followed directly by a name; an operator or `;` is missing here"
                .to_string(),
        );
    }
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

fn suggested_keyword(expected: &[Expected], found: Token) -> Option<String> {
    let accepts_a_name = expected
        .iter()
        .any(|alternative| alternative.accepts(TokenKind::Identifier));
    if found.kind != TokenKind::Identifier || accepts_a_name {
        return None;
    }
    let written = SrcMap::text_of(found.span)?;

    expected
        .iter()
        .filter_map(Expected::spelling)
        .filter(|spelling| is_word_spelling(spelling) && is_probable_typo_of(&written, spelling))
        .min_by_key(|spelling| edit_distance(&written, spelling))
        .map(str::to_string)
}

fn is_probable_typo_of(written: &str, keyword: &str) -> bool {
    if written.len() >= 2 && keyword.starts_with(written) {
        return true;
    }
    let allowed = if written.len() >= 4 { 2 } else { 1 };
    edit_distance(written, keyword) <= allowed
}

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
    STATEMENT_STARTERS.contains(&kind)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn alternatives(kinds: &[TokenKind]) -> Vec<Expected> {
        kinds.iter().map(|kind| Expected::of_kind(*kind)).collect()
    }

    #[test]
    fn a_short_list_is_enumerated() {
        let expected = vec![
            Expected::of_kind(TokenKind::Arrow),
            Expected::decode("a block"),
            Expected::of_kind(TokenKind::Semicolon),
        ];
        let rendered = render_alternatives(&expected).expect("three alternatives are named");
        assert_eq!(rendered, "`->`, a block, or `;`");
    }

    #[test]
    fn a_long_list_of_operators_keeps_the_terminators() {
        let mut expected = vec![Expected::decode("an expression")];
        for kind in [
            TokenKind::Plus,
            TokenKind::Minus,
            TokenKind::Star,
            TokenKind::Slash,
            TokenKind::Semicolon,
            TokenKind::Comma,
        ] {
            expected.push(Expected::of_kind(kind));
        }
        let rendered = render_alternatives(&expected).expect("kept alternatives are named");
        assert_eq!(rendered, "an expression, `;`, or `,`");
    }

    #[test]
    fn a_set_of_only_item_keywords_is_not_truncated() {
        let expected = alternatives(&[
            TokenKind::PublicKw,
            TokenKind::FunKw,
            TokenKind::StructKw,
            TokenKind::EnumKw,
            TokenKind::TraitKw,
            TokenKind::ExtendKw,
            TokenKind::ModuleKw,
            TokenKind::ImportKw,
        ]);
        let rendered = render_alternatives(&expected).expect("all eight keywords are named");
        assert!(rendered.contains("`import`"));
        assert!(rendered.contains("`public`"));
        assert!(!rendered.contains("other kinds of token"));
    }

    #[test]
    fn transpositions_count_as_one_edit() {
        assert_eq!(edit_distance("strcut", "struct"), 1);
    }

    #[test]
    fn a_two_character_prefix_of_a_keyword_is_a_probable_typo() {
        assert!(is_probable_typo_of("pub", "public"));
        assert!(!is_probable_typo_of("xyz", "public"));
    }
}
