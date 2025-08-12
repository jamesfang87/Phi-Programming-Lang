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
  EOFKind,
  /// Error token for invalid inputs
  ErrorKind,

  // KEYWORDS
  BoolKwKind,     ///< `bool` keyword
  BreakKwKind,    ///< `break` keyword
  ConstKwKind,    ///< `const` keyword
  ContinueKwKind, ///< `continue` keyword
  ElseKwKind,     ///< `else` keyword
  EnumKwKind,
  FalseKwKind,  ///< `false` literal
  ForKwKind,    ///< `for` keyword
  FunKwKind,    ///< `fun` keyword
  IfKwKind,     ///< `if` keyword
  ImportKwKind, ///< `import` keyword
  InKwKind,     ///< `in` keyword
  PublicKwKind, ///< `public` keyword
  ReturnKwKind, ///< `return` keyword
  StructKwKind,
  TrueKwKind,  ///< `true` literal
  VarKwKind,   ///< `let` keyword
  WhileKwKind, ///< `while` keyword

  // SIGNED INTEGER TYPES
  I8Kind,  ///< `i8` type (8-bit signed)
  I16Kind, ///< `i16` type (16-bit signed)
  I32Kind, ///< `i32` type (32-bit signed)
  I64Kind, ///< `i64` type (64-bit signed)

  // UNSIGNED INTEGER TYPES
  U8Kind,  ///< `u8` type (8-bit unsigned)
  U16Kind, ///< `u16` type (16-bit unsigned)
  U32Kind, ///< `u32` type (32-bit unsigned)
  U64Kind, ///< `u64` type (64-bit unsigned)

  // FLOATING-POINT TYPES
  F32Kind, ///< `f32` type (32-bit float)
  F64Kind, ///< `f64` type (64-bit float)

  // TEXT TYPES
  StringKind, ///< `str` type keyword
  CharKind,   ///< `char` type keyword

  // SYNTAX DELIMITERS
  OpenParenKind,    ///< `(` parenthesis
  CloseParenKind,   ///< `)` parenthesis
  OpenBraceKind,    ///< `{` brace
  CloseBraceKind,   ///< `}` brace
  OpenBracketKind,  ///< `[` bracket
  CloseBracketKind, ///< `]` bracket
  ArrowKind,        ///< `->` function return
  CommaKind,        ///< `,` separator
  SemicolonKind,    ///< `;` statement terminator

  // OPERATORS
  PlusKind,    ///< `+` addition
  MinusKind,   ///< `-` subtraction/negation
  StarKind,    ///< `*` multiplication
  SlashKind,   ///< `/` division
  PercentKind, ///< `%` modulo
  BangKind,    ///< `!` logical NOT

  // COMPOUND ASSIGNMENT OPERATORS
  PlusEqualsKind, ///< `+=` add-assign
  SubEqualsKind,  ///< `-=` subtract-assign
  MulEqualKind,   ///< `*=` multiply-assign
  DivEqualsKind,  ///< `/=` divide-assign
  ModEqualsKind,  ///< `%=` modulo-assign

  // MEMBER ACCESS
  PeriodKind,      ///< `.` member access
  DoubleColonKind, ///< `::` namespace access

  // INCREMENT/DECREMENT
  DoublePlusKind,  ///< `++` increment
  DoubleMinusKind, ///< `--` decrement

  // EQUALITY
  DoubleEqualsKind, ///< `==` equality
  BangEqualsKind,   ///< `!=` inequality

  // LOGICAL
  DoubleAmpKind,  ///< `&&` logical AND
  DoublePipeKind, ///< `||` logical OR

  // RELATIONAL
  OpenCaretKind,    ///< `<` less than
  LessEqualKind,    ///< `<=` less than or equal
  CloseCaretKind,   ///< `>` greater than
  GreaterEqualKind, ///< `>=` greater than or equal

  // ASSIGNMENT AND TYPE
  EqualsKind, ///< `=` assignment
  ColonKind,  ///< `:` type annotation

  // RANGE OPERATORS
  ExclRangeKind, ///< `..` exclusive range
  InclRangeKind, ///< `..=` inclusive range

  // LITERALS
  IntLiteralKind,   ///< Integer literal (e.g., 42)
  FloatLiteralKind, ///< Float literal (e.g., 3.14)
  StrLiteralKind,   ///< String literal (e.g., "text")
  CharLiteralKind,  ///< Char literal (e.g., 'a')
  IdentifierKind,   ///< Identifier (user-defined names)
};

/**
 * @brief Converts TokenType to human-readable string
 * @param Type Token type to convert
 * @return String representation of token type
 */
std::string tyToStr(TokenKind Type);

} // namespace phi
