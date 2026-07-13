use crate::lexer::src_span::SrcSpan;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenKind {
    Eof, // End of file marker

    // KEYWORDS
    AnyKw,        // `any` keyword
    AsKw,         // `as` keyword
    BoolKw,       // `bool` keyword
    BreakKw,      // `break` keyword
    ConcurrentKw, // `concurrent` keyword
    ContinueKw,   // `continue` keyword
    DeferKw,      // `defer` keyword
    DynKw,        // `dyn` keyword
    ElseKw,       // `else` keyword
    EnumKw,       // `enum` keyword
    ExtendKw,     // `extend` keyword
    FalseKw,      // `false` literal
    ForKw,        // `for` keyword
    FunKw,        // `fun` keyword
    IfKw,         // `if` keyword
    ImportKw,     // `import` keyword
    InKw,         // `in` keyword
    LetKw,        // `let` keyword
    MatchKw,      // `match` keyword
    ModuleKw,     // `module` keyword
    MutKw,        // `mut` keyword
    PublicKw,     // `public` keyword
    ReturnKw,     // `return` keyword
    LowerSelfKw,  // `self` keyword
    UpperSelfKw,  // `Self` keyword
    SpawnKw,      // `spawn` keyword
    StructKw,     // `struct` keyword
    TraitKw,      // `trait`keyword
    TrueKw,       // `true` literal
    UseKw,        // `use` keyword
    WhileKw,      // `while` keyword
    WithKw,       // `with` keyword

    // INTRINSICS
    Panic,
    Assert,
    Unreachable,
    TypeOf,

    // SIGNED INTEGER TYPES
    I8,  // `i8` type (8-bit signed)
    I16, // `i16` type (16-bit signed)
    I32, // `i32` type (32-bit signed)
    I64, // `i64` type (64-bit signed)
    // UNSIGNED INTEGER TYPES
    U8,  // `u8` type (8-bit unsigned)
    U16, // `u16` type (16-bit unsigned)
    U32, // `u32` type (32-bit unsigned)
    U64, // `u64` type (64-bit unsigned)
    // FLOATING-POINT TYPES
    F32, // `f32` type (32-bit float)
    F64, // `f64` type (64-bit float)
    // TEXT TYPES
    String, // `str` type keyword
    Char,   // `char` type keyword
    // SYNTAX DELIMITERS
    OpenParen,    // `(` parenthesis
    CloseParen,   // `)` parenthesis
    OpenBrace,    // `{` brace
    CloseBrace,   // `}` brace
    OpenBracket,  // `[` bracket
    CloseBracket, // `]` bracket
    Arrow,        // `->` function return
    FatArrow,     // `=>` as in the match stmt
    Comma,        // `,` separator
    Semicolon,    // `;` statement terminator

    // OPERATORS
    Plus,    // `+` addition
    Minus,   // `-` subtraction/negation
    Star,    // `*` multiplication
    Slash,   // `/` division
    Percent, // `%` modulo
    Bang,    // `!` logical NOT
    Amp,     // `&` references
    Try,     // `?` short-hand for Result/Option

    // COMPOUND ASSIGNMENT OPERATORS
    PlusEquals, // `+=` add-assign
    SubEquals,  // `-=` subtract-assign
    MulEquals,  // `*=` multiply-assign
    DivEquals,  // `/=` divide-assign
    ModEquals,  // `%=` modulo-assign

    // MEMBER ACCESS
    Period,      // `.` member access
    DoubleColon, // `::` namespace/enum access

    // INCREMENT/DECREMENT
    DoublePlus,  // `++` increment
    DoubleMinus, // `--` decrement

    // EQUALITY
    DoubleEquals, // `==` equality
    BangEquals,   // `!=` inequality

    // LOGICAL
    DoubleAmp,  // `&&` logical AND
    DoublePipe, // `||` logical OR

    // ALTERNATION
    Pipe, // `|` alternation in patterns

    // RELATIONAL
    OpenCaret,    // `<` less than
    LessEqual,    // `<=` less than or equal
    CloseCaret,   // `>` greater than
    GreaterEqual, // `>=` greater than or equal

    // ASSIGNMENT AND TYPE
    Equals, // `=` assignment
    Colon,  // `:` type annotation

    // RANGE OPERATORS
    ExclRange, // `..` exclusive range
    InclRange, // `..=` inclusive range

    // WILDCARD
    Wildcard, // `_`

    // LITERALS
    IntLiteral,   // Integer literal (e.g., 42)
    FloatLiteral, // Float literal (e.g., 3.14)
    StrLiteral,   // String literal (e.g., "text")
    CharLiteral,  // Char literal (e.g., 'a')
    Identifier,   // Identifier (user-defined names)
}

impl TokenKind {
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

    /// Returns `true` if the token is an arithmetic operator (`+`, `-`, `*`, `/`, `%`).
    pub fn is_arithmetic(self) -> bool {
        matches!(
            self,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Star
                | TokenKind::Slash
                | TokenKind::Percent
        )
    }

    /// Returns `true` if the token is a logical operator (`!`, `&&`, `||`).
    pub fn is_logical(self) -> bool {
        matches!(
            self,
            TokenKind::Bang | TokenKind::DoubleAmp | TokenKind::DoublePipe
        )
    }

    /// Returns `true` if the token is a comparison operator (`<`, `<=`, `>`, `>=`).
    pub fn is_comparison(self) -> bool {
        matches!(
            self,
            TokenKind::OpenCaret
                | TokenKind::LessEqual
                | TokenKind::CloseCaret
                | TokenKind::GreaterEqual
        )
    }

    /// Returns `true` if the token is an equality operator (`==`, `!=`).
    pub fn is_equality(self) -> bool {
        matches!(self, TokenKind::DoubleEquals | TokenKind::BangEquals)
    }
}

impl std::fmt::Display for TokenKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // NB: `self.to_string()` here would resolve to the blanket
        // `ToString::to_string(&self)` (an exact match on `&TokenKind`) instead
        // of the inherent by-value `to_string` above, calling back into this
        // `fmt` and recursing forever. The explicit UFCS call sidesteps that.
        f.write_str(TokenKind::to_string(*self))
    }
}

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
