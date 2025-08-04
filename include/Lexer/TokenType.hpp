#pragma once

#include <string>

namespace phi {

/**
 * @brief Enumeration of token types for Phi language
 *
 * Defines all lexical tokens recognized by the scanner, including:
 * - Keywords and type specifiers
 * - Operators and punctuation
 * - Literal value tokens
 * - Structural syntax elements
 *
 * Uses enum class for type safety and namespace protection.
 */
enum class TokenKind : uint8_t {
  /// End of file marker
  tokEOF,
  /// Error token for invalid inputs
  tokError,

  // KEYWORDS
  tokBool,     ///< `bool` keyword
  tokBreak,    ///< `break` keyword
  tokClass,    ///< `class` keyword
  tokConst,    ///< `const` keyword
  tokContinue, ///< `continue` keyword
  tokElse,     ///< `else` keyword
  tokFalse,    ///< `false` literal
  tokFor,      ///< `for` keyword
  tokFun,      ///< `fun` keyword
  tokIf,       ///< `if` keyword
  tokImport,   ///< `import` keyword
  tokIn,       ///< `in` keyword
  tokLet,      ///< `let` keyword
  tokReturn,   ///< `return` keyword
  tokTrue,     ///< `true` literal
  tokWhile,    ///< `while` keyword

  // SIGNED INTEGER TYPES
  tokI8,  ///< `i8` type (8-bit signed)
  tokI16, ///< `i16` type (16-bit signed)
  tokI32, ///< `i32` type (32-bit signed)
  tokI64, ///< `i64` type (64-bit signed)

  // UNSIGNED INTEGER TYPES
  tokU8,  ///< `u8` type (8-bit unsigned)
  tokU16, ///< `u16` type (16-bit unsigned)
  tokU32, ///< `u32` type (32-bit unsigned)
  tokU64, ///< `u64` type (64-bit unsigned)

  // FLOATING-POINT TYPES
  tokF32, ///< `f32` type (32-bit float)
  tokF64, ///< `f64` type (64-bit float)

  // TEXT TYPES
  tokStr,  ///< `str` type keyword
  tokChar, ///< `char` type keyword

  // SYNTAX DELIMITERS
  tokOpenParen,    ///< `(` parenthesis
  tokRightParen,   ///< `)` parenthesis
  tokLeftBrace,    ///< `{` brace
  tokRightBrace,   ///< `}` brace
  tokLeftBracket,  ///< `[` bracket
  tokRightBracket, ///< `]` bracket
  tokArrow,        ///< `->` function return
  tokComma,        ///< `,` separator
  tokSemicolon,    ///< `;` statement terminator

  // OPERATORS
  tokPlus,    ///< `+` addition
  tokMinus,   ///< `-` subtraction/negation
  tokStar,    ///< `*` multiplication
  tokSlash,   ///< `/` division
  tokPercent, ///< `%` modulo
  tokBang,    ///< `!` logical NOT

  // COMPOUND ASSIGNMENT OPERATORS
  tokPlusEquals, ///< `+=` add-assign
  tokSubEquals,  ///< `-=` subtract-assign
  tokMulEquals,  ///< `*=` multiply-assign
  tokDivEquals,  ///< `/=` divide-assign
  tokModEquals,  ///< `%=` modulo-assign

  // MEMBER ACCESS
  tokPeriod,      ///< `.` member access
  tokDoubleColon, ///< `::` namespace access

  // INCREMENT/DECREMENT
  tokDoublePlus,  ///< `++` increment
  tokDoubleMinus, ///< `--` decrement

  // EQUALITY
  tokDoubleEquals, ///< `==` equality
  tokBangEquals,   ///< `!=` inequality

  // LOGICAL
  tokDoubleAmp,  ///< `&&` logical AND
  tokDoublePipe, ///< `||` logical OR

  // RELATIONAL
  tokLeftCaret,    ///< `<` less than
  tokLessEqual,    ///< `<=` less than or equal
  tokRightCaret,   ///< `>` greater than
  tokGreaterEqual, ///< `>=` greater than or equal

  // ASSIGNMENT AND TYPE
  tokEquals, ///< `=` assignment
  tokColon,  ///< `:` type annotation

  // RANGE OPERATORS
  tokExclusiveRange, ///< `..` exclusive range
  tokInclusiveRange, ///< `..=` inclusive range

  // LITERALS
  tokIntLiteral,   ///< Integer literal (e.g., 42)
  tokFloatLiteral, ///< Float literal (e.g., 3.14)
  tokStrLiteral,   ///< String literal (e.g., "text")
  tokCharLiteral,  ///< Char literal (e.g., 'a')
  tokIdentifier,   ///< Identifier (user-defined names)
};

/**
 * @brief Converts TokenType to human-readable string
 * @param type Token type to convert
 * @return String representation of token type
 */
std::string tyToStr(TokenKind type);

} // namespace phi
