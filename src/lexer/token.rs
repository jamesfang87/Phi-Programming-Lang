//! [`Token`] represents a single lexical token in the Phi Programming Language;
//! it consists of two part:, a [`TokenKind`] that records what type of token
//! it is and a [`SrcSpan`].

use crate::driver::source::SrcSpan;

/// [`TokenKind`] is the kind of a single token. See [`TokenKind::to_string`]
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
    LetKw,
    MatchKw,
    ModuleKw,
    MutKw,
    PublicKw,
    ReturnKw,
    LowerSelfKw, // `self`
    UpperSelfKw, // `Self`
    SpawnKw,
    StructKw,
    TraitKw,
    TrueKw,
    UseKw,
    WhileKw,
    WithKw,

    // INTRINSICS
    Panic,
    Assert,
    Unreachable,
    TypeOf,

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
    // FLOATING-POINT TYPES
    F32,
    F64,
    // TEXT TYPES
    String, // `str` type keyword
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
    SubEquals,
    MulEquals,
    DivEquals,
    ModEquals,

    // MEMBER ACCESS
    Period,
    DoubleColon, // namespace or enum variant access

    // INCREMENT/DECREMENT
    DoublePlus,
    DoubleMinus,

    // EQUALITY
    DoubleEquals,
    BangEquals,

    // LOGICAL
    DoubleAmp,
    DoublePipe,

    // ALTERNATION
    Pipe, // pattern alternation, e.g. `1 | 2 | 3`

    // RELATIONAL
    OpenCaret, // `<`
    LessEqual,
    CloseCaret, // `>`
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

impl TokenKind {
    /// Returns the source text this kind corresponds to, or a short description for kinds that
    /// stand for a class of lexemes rather than one fixed spelling (e.g. `"identifier"`).
    ///
    /// Used to build diagnostic messages like "expected `;`, found `foo`".
    pub fn to_string(self) -> &'static str {
        match self {
            TokenKind::Eof => "end of file",
            TokenKind::AnyKw => "any",
            TokenKind::AsKw => "as",
            TokenKind::BoolKw => "bool",
            TokenKind::BreakKw => "break",
            TokenKind::ConcurrentKw => "concurrent",
            TokenKind::ContinueKw => "continue",
            TokenKind::DeferKw => "defer",
            TokenKind::DynKw => "dyn",
            TokenKind::ElseKw => "else",
            TokenKind::EnumKw => "enum",
            TokenKind::ExtendKw => "extend",
            TokenKind::FalseKw => "false",
            TokenKind::ForKw => "for",
            TokenKind::FunKw => "fun",
            TokenKind::IfKw => "if",
            TokenKind::ImportKw => "import",
            TokenKind::InKw => "in",
            TokenKind::LetKw => "let",
            TokenKind::MatchKw => "match",
            TokenKind::ModuleKw => "module",
            TokenKind::MutKw => "mut",
            TokenKind::PublicKw => "public",
            TokenKind::ReturnKw => "return",
            TokenKind::LowerSelfKw => "self",
            TokenKind::UpperSelfKw => "Self",
            TokenKind::SpawnKw => "spawn",
            TokenKind::StructKw => "struct",
            TokenKind::TraitKw => "trait",
            TokenKind::TrueKw => "true",
            TokenKind::UseKw => "use",
            TokenKind::WhileKw => "while",
            TokenKind::WithKw => "with",
            TokenKind::Panic => "panic",
            TokenKind::Assert => "assert",
            TokenKind::Unreachable => "unreachable",
            TokenKind::TypeOf => "typeof",
            TokenKind::I8 => "i8",
            TokenKind::I16 => "i16",
            TokenKind::I32 => "i32",
            TokenKind::I64 => "i64",
            TokenKind::U8 => "u8",
            TokenKind::U16 => "u16",
            TokenKind::U32 => "u32",
            TokenKind::U64 => "u64",
            TokenKind::F32 => "f32",
            TokenKind::F64 => "f64",
            TokenKind::String => "str",
            TokenKind::Char => "char",
            TokenKind::OpenParen => "(",
            TokenKind::CloseParen => ")",
            TokenKind::OpenBrace => "{",
            TokenKind::CloseBrace => "}",
            TokenKind::OpenBracket => "[",
            TokenKind::CloseBracket => "]",
            TokenKind::Arrow => "->",
            TokenKind::FatArrow => "=>",
            TokenKind::Comma => ",",
            TokenKind::Semicolon => ";",
            TokenKind::Plus => "+",
            TokenKind::Minus => "-",
            TokenKind::Star => "*",
            TokenKind::Slash => "/",
            TokenKind::Percent => "%",
            TokenKind::Bang => "!",
            TokenKind::Amp => "&",
            TokenKind::Try => "?",
            TokenKind::PlusEquals => "+=",
            TokenKind::SubEquals => "-=",
            TokenKind::MulEquals => "*=",
            TokenKind::DivEquals => "/=",
            TokenKind::ModEquals => "%=",
            TokenKind::Period => ".",
            TokenKind::DoubleColon => "::",
            TokenKind::DoublePlus => "++",
            TokenKind::DoubleMinus => "--",
            TokenKind::DoubleEquals => "==",
            TokenKind::BangEquals => "!=",
            TokenKind::DoubleAmp => "&&",
            TokenKind::DoublePipe => "||",
            TokenKind::Pipe => "|",
            TokenKind::OpenCaret => "<",
            TokenKind::LessEqual => "<=",
            TokenKind::CloseCaret => ">",
            TokenKind::GreaterEqual => ">=",
            TokenKind::Equals => "=",
            TokenKind::Colon => ":",
            TokenKind::ExclRange => "..",
            TokenKind::InclRange => "..=",
            TokenKind::Wildcard => "_",
            TokenKind::IntLiteral => "integer literal",
            TokenKind::FloatLiteral => "float literal",
            TokenKind::StrLiteral => "string literal",
            TokenKind::CharLiteral => "char literal",
            TokenKind::Identifier => "identifier",
        }
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Do not call `self.to_string()` here. Rust would resolve it to the blanket
        // `ToString::to_string(&self)` instead of the inherent method above, because it is an
        // exact match on `&TokenKind`. That calls back into this `fmt` and recurses forever.
        // The explicit UFCS call below picks the inherent method and avoids the loop.
        f.write_str(TokenKind::to_string(*self))
    }
}

/// [`Token`] represents a single lexical token. It includes the type of token
/// and its source span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: SrcSpan,
}

impl std::fmt::Display for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}
