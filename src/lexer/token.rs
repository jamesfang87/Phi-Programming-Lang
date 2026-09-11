//! [`Token`] represents a single lexical token in the Phi Programming Language;
//! it consists of two parts: a [`TokenKind`] that records what type of token
//! it is and a [`SrcSpan`].

use crate::driver::source::{SrcMap, SrcSpan};
use crate::lexer::describe::Descriptor;

/// [`Token`] represents a single lexical token. It includes the type of token
/// and its source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SrcSpan,
}

impl Token {
    pub fn text(&self) -> String {
        SrcMap::text_of(self.span).expect("lexer token span should always resolve to a source file")
    }
}

/// [`TokenKind`] is the kind of a single token. See [`crate::lexer::describe`]
/// for the exact source text each variant corresponds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof,

    // KEYWORDS
    AnyKw,
    AsKw,
    BoolKw,
    BreakKw,
    ConcurrentKw,
    ContinueKw,
    DeferKw,
    DynKw,
    ElseKw,
    EnumKw,
    ExtendKw,
    FalseKw,
    ForKw,
    FunKw,
    IfKw,
    ImportKw,
    InKw,
    IsoKw,
    LetKw,
    MatchKw,
    ModuleKw,
    MutKw,
    NewKw,
    PublicKw,
    ReturnKw,
    LowerSelfKw, // `self`
    UpperSelfKw, // `Self`
    SpawnKw,
    StructKw,
    TraitKw,
    TrueKw,
    WhileKw,
    WithKw,

    // INTRINSICS
    Panic,
    Assert,
    Unreachable,

    // SIGNED INTEGER TYPES
    I8,
    I16,
    I32,
    I64,
    // UNSIGNED INTEGER TYPES
    U8,
    U16,
    U32,
    U64,
    Usize,
    // FLOATING-POINT TYPES
    F32,
    F64,
    // TEXT TYPES
    Str, // `str`
    Char,

    // SYNTAX DELIMITERS
    OpenParen,
    CloseParen,
    OpenBrace,
    CloseBrace,
    OpenBracket,
    CloseBracket,
    Arrow,    // `->`, function return type
    FatArrow, // `=>`, match arms
    Comma,
    Semicolon,

    // OPERATORS
    Plus,
    Minus, // subtraction or unary negation
    Star,
    Slash,
    Percent,
    Bang, // logical NOT
    Amp,  // references
    Try,  // `?`, Result/Option short-circuit

    // COMPOUND ASSIGNMENT OPERATORS
    PlusEquals,
    MinusEquals,
    MulEquals,
    DivEquals,
    ModEquals,

    // MEMBER ACCESS
    Period,
    DoubleColon, // namespace or enum variant access

    // EQUALITY
    DoubleEquals,
    BangEquals,

    // LOGICAL
    DoubleAmp,
    DoublePipe,

    // ALTERNATION
    Pipe, // closure parameter lists, e.g. `|x| x + 1`

    // RELATIONAL
    OpenAngle, // `<`
    LessEqual,
    CloseAngle, // `>`
    GreaterEqual,

    // ASSIGNMENT AND TYPE
    Equals,
    Colon,

    // RANGE OPERATORS
    ExclRange, // `..`
    InclRange, // `..=`

    // WILDCARD
    Wildcard, // `_`

    // LITERALS
    IntLiteral,
    FloatLiteral,
    StrLiteral,
    CharLiteral,
    Identifier,
}

pub(crate) const KEYWORDS: &[(&str, TokenKind)] = &[
    ("any", TokenKind::AnyKw),
    ("as", TokenKind::AsKw),
    ("bool", TokenKind::BoolKw),
    ("break", TokenKind::BreakKw),
    ("concurrent", TokenKind::ConcurrentKw),
    ("continue", TokenKind::ContinueKw),
    ("defer", TokenKind::DeferKw),
    ("dyn", TokenKind::DynKw),
    ("else", TokenKind::ElseKw),
    ("enum", TokenKind::EnumKw),
    ("extend", TokenKind::ExtendKw),
    ("false", TokenKind::FalseKw),
    ("for", TokenKind::ForKw),
    ("fun", TokenKind::FunKw),
    ("if", TokenKind::IfKw),
    ("import", TokenKind::ImportKw),
    ("in", TokenKind::InKw),
    ("iso", TokenKind::IsoKw),
    ("let", TokenKind::LetKw),
    ("match", TokenKind::MatchKw),
    ("module", TokenKind::ModuleKw),
    ("mut", TokenKind::MutKw),
    ("new", TokenKind::NewKw),
    ("public", TokenKind::PublicKw),
    ("return", TokenKind::ReturnKw),
    ("self", TokenKind::LowerSelfKw),
    ("Self", TokenKind::UpperSelfKw),
    ("spawn", TokenKind::SpawnKw),
    ("struct", TokenKind::StructKw),
    ("trait", TokenKind::TraitKw),
    ("true", TokenKind::TrueKw),
    ("while", TokenKind::WhileKw),
    ("with", TokenKind::WithKw),
    ("i8", TokenKind::I8),
    ("i16", TokenKind::I16),
    ("i32", TokenKind::I32),
    ("i64", TokenKind::I64),
    ("u8", TokenKind::U8),
    ("u16", TokenKind::U16),
    ("u32", TokenKind::U32),
    ("u64", TokenKind::U64),
    ("usize", TokenKind::Usize),
    ("f32", TokenKind::F32),
    ("f64", TokenKind::F64),
    ("str", TokenKind::Str),
    ("char", TokenKind::Char),
    ("panic", TokenKind::Panic),
    ("assert", TokenKind::Assert),
    ("unreachable", TokenKind::Unreachable),
];

impl TokenKind {
    /// Whether this kind opens a `(`, `[` or `{` group.
    pub fn opens_group(self) -> bool {
        matches!(
            self,
            TokenKind::OpenParen | TokenKind::OpenBracket | TokenKind::OpenBrace
        )
    }

    /// Whether this kind closes a `(`, `[` or `{` group.
    pub fn closes_group(self) -> bool {
        matches!(
            self,
            TokenKind::CloseParen | TokenKind::CloseBracket | TokenKind::CloseBrace
        )
    }
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(Descriptor::of(*self).name())
    }
}

/// The kinds that can begin a statement.
///
/// The parser's statement recovery stops before these, and the parser's diagnostics use the
/// same list to recognize a statement that starts where the previous one should have ended.
pub(crate) const STATEMENT_STARTERS: &[TokenKind] = &[
    TokenKind::LetKw,
    TokenKind::ReturnKw,
    TokenKind::WhileKw,
    TokenKind::ForKw,
    TokenKind::IfKw,
    TokenKind::MatchKw,
    TokenKind::BreakKw,
    TokenKind::ContinueKw,
    TokenKind::DeferKw,
    TokenKind::WithKw,
];

/// The kinds that can begin a top-level item.
///
/// The parser's item recovery stops before these; the parser's tests use the list to check
/// that every kind really does open an item, so the two cannot drift apart.
pub(crate) const ITEM_STARTERS: &[TokenKind] = &[
    TokenKind::PublicKw,
    TokenKind::FunKw,
    TokenKind::StructKw,
    TokenKind::EnumKw,
    TokenKind::TraitKw,
    TokenKind::ExtendKw,
    TokenKind::ModuleKw,
    TokenKind::ImportKw,
];
