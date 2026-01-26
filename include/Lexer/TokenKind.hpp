#pragma once

#include <cstdint>
#include <string>

namespace phi {

struct TokenKind {
  enum Kind : uint8_t {
    /// End of file marker
    Eof,
    /// Error token for invalid inputs
    Error,

    // KEYWORDS
    AsKw,       ///< `as` keyword
    BoolKw,     ///< `bool` keyword
    BreakKw,    ///< `break` keyword
    ConstKw,    ///< `const` keyword
    ContinueKw, ///< `continue` keyword
    DeferKw,    ///< `defer` keyword
    ElseKw,     ///< `else` keyword
    EnumKw,     ///< `enum` keyword
    FalseKw,    ///< `false` literal
    ForKw,      ///< `for` keyword
    FunKw,      ///< `fun` keyword
    IfKw,       ///< `if` keyword
    ImportKw,   ///< `import` keyword
    InKw,       ///< `in` keyword
    MatchKw,    ///< `match` keyword
    ModuleKw,   ///< `module` keyword
    PublicKw,   ///< `public` keyword
    ReturnKw,   ///< `return` keyword
    StructKw,   ///< `struct` keyword
    TrueKw,     ///< `true` literal
    ThisKw,     ///< `this` keyword
    VarKw,      ///< `let` keyword
    WhileKw,    ///< `while` keyword

    // INTRINSICS
    Panic,
    Assert,
    Unreachable,
    TypeOf,

    // SIGNED INTEGER TYPES
    I8,  ///< `i8` type (8-bit signed)
    I16, ///< `i16` type (16-bit signed)
    I32, ///< `i32` type (32-bit signed)
    I64, ///< `i64` type (64-bit signed)

    // UNSIGNED INTEGER TYPES
    U8,  ///< `u8` type (8-bit unsigned)
    U16, ///< `u16` type (16-bit unsigned)
    U32, ///< `u32` type (32-bit unsigned)
    U64, ///< `u64` type (64-bit unsigned)

    // FLOATING-POINT TYPES
    F32, ///< `f32` type (32-bit float)
    F64, ///< `f64` type (64-bit float)

    // TEXT TYPES
    String, ///< `str` type keyword
    Char,   ///< `char` type keyword

    // SYNTAX DELIMITERS
    OpenParen,    ///< `(` parenthesis
    CloseParen,   ///< `)` parenthesis
    OpenBrace,    ///< `{` brace
    CloseBrace,   ///< `}` brace
    OpenBracket,  ///< `[` bracket
    CloseBracket, ///< `]` bracket
    Arrow,        ///< `->` function return
    FatArrow,     ///< `=>` as in the match stmt
    Comma,        ///< `,` separator
    Semicolon,    ///< `;` statement terminator

    // OPERATORS
    Plus,    ///< `+` addition
    Minus,   ///< `-` subtraction/negation
    Star,    ///< `*` multiplication
    Slash,   ///< `/` division
    Percent, ///< `%` modulo
    Bang,    ///< `!` logical NOT
    Amp,     ///< `&` references
    Try,     ///< `?` short-hand for Result/Option

    // COMPOUND ASSIGNMENT OPERATORS
    PlusEquals, ///< `+=` add-assign
    SubEquals,  ///< `-=` subtract-assign
    MulEquals,  ///< `*=` multiply-assign
    DivEquals,  ///< `/=` divide-assign
    ModEquals,  ///< `%=` modulo-assign

    // MEMBER ACCESS
    Period,      ///< `.` member access
    DoubleColon, ///< `::` namespace/enum access

    // INCREMENT/DECREMENT
    DoublePlus,  ///< `++` increment
    DoubleMinus, ///< `--` decrement

    // EQUALITY
    DoubleEquals, ///< `==` equality
    BangEquals,   ///< `!=` inequality

    // LOGICAL
    DoubleAmp,  ///< `&&` logical AND
    DoublePipe, ///< `||` logical OR

    // ALTERNATION
    Pipe, ///< `|` alternation in patterns

    // RELATIONAL
    OpenCaret,    ///< `<` less than
    LessEqual,    ///< `<=` less than or equal
    CloseCaret,   ///< `>` greater than
    GreaterEqual, ///< `>=` greater than or equal

    // ASSIGNMENT AND TYPE
    Equals, ///< `=` assignment
    Colon,  ///< `:` type annotation

    // RANGE OPERATORS
    ExclRange, ///< `..` exclusive range
    InclRange, ///< `..=` inclusive range

    // WILDCARD
    Wildcard, ///< `_`

    // LITERALS
    IntLiteral,   ///< Integer literal (e.g., 42)
    FloatLiteral, ///< Float literal (e.g., 3.14)
    StrLiteral,   ///< String literal (e.g., "text")
    CharLiteral,  ///< Char literal (e.g., 'a')
    Identifier,   ///< Identifier (user-defined names)
  };
  Kind Value;

  std::string toString() const;
  [[nodiscard]] bool isArithmetic() const noexcept;
  [[nodiscard]] bool isLogical() const noexcept;
  [[nodiscard]] bool isComparison() const noexcept;
  [[nodiscard]] bool isEquality() const noexcept;

  constexpr operator Kind() const noexcept { return Value; }
  bool operator==(const TokenKind &Rhs) { return Value == Rhs.Value; }
  bool operator!=(const TokenKind &Rhs) { return !(Value == Rhs.Value); }
};

} // namespace phi
