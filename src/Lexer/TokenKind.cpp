#include "Lexer/TokenKind.hpp"

namespace phi {

/**
 * @brief Converts a TokenType enumeration value to its string representation
 *
 * This function provides a mapping from TokenType enum values to their
 * corresponding human-readable string names. This is primarily used for
 * debugging output and error messages. The returned strings are in uppercase
 * and match the token type names without the "tok_" prefix.
 *
 * @param Type The TokenType to convert
 * @return A string representation of the token type (e.g., "IDENTIFIER", "ADD")
 */

std::string tyToStr(const TokenKind Kind) {
  switch (Kind) {
  // Special tokens
  case TokenKind::EOFKind:
    return "EOF";
  case TokenKind::ErrorKind:
    return "ERROR";

  // Keywords
  case TokenKind::BoolKwKind:
    return "BOOL";
  case TokenKind::BreakKwKind:
    return "BREAK";
  case TokenKind::ConstKwKind:
    return "CONST";
  case TokenKind::ContinueKwKind:
    return "CONTINUE";
  case TokenKind::ElseKwKind:
    return "ELSE";
  case TokenKind::EnumKwKind:
    return "ENUM";
  case TokenKind::FalseKwKind:
    return "FALSE";
  case TokenKind::ForKwKind:
    return "FOR";
  case TokenKind::FunKwKind:
    return "FUN";
  case TokenKind::IfKwKind:
    return "IF";
  case TokenKind::ImportKwKind:
    return "IMPORT";
  case TokenKind::InKwKind:
    return "IN";
  case TokenKind::VarKwKind:
    return "VAR";
  case TokenKind::ReturnKwKind:
    return "RETURN";
  case TokenKind::StructKwKind:
    return "STRUCT";
  case TokenKind::TrueKwKind:
    return "TRUE";
  case TokenKind::WhileKwKind:
    return "WHILE";

  // Signed integer types
  case TokenKind::I8Kind:
    return "I8";
  case TokenKind::I16Kind:
    return "I16";
  case TokenKind::I32Kind:
    return "I32";
  case TokenKind::I64Kind:
    return "I64";

  // Unsigned integer types
  case TokenKind::U8Kind:
    return "U8";
  case TokenKind::U16Kind:
    return "U16";
  case TokenKind::U32Kind:
    return "U32";
  case TokenKind::U64Kind:
    return "U64";

  // Floating point types
  case TokenKind::F32Kind:
    return "F32";
  case TokenKind::F64Kind:
    return "F64";

  // Text types
  case TokenKind::StringKind:
    return "STRING";
  case TokenKind::CharKind:
    return "CHAR";

  // Syntax elements
  case TokenKind::OpenParenKind:
    return "OPEN_PAREN";
  case TokenKind::CloseParenKind:
    return "CLOSE_PAREN";
  case TokenKind::OpenBraceKind:
    return "OPEN_BRACE";
  case TokenKind::CloseBraceKind:
    return "CLOSE_BRACE";
  case TokenKind::OpenBracketKind:
    return "OPEN_BRACKET";
  case TokenKind::CloseBracketKind:
    return "CLOSE_BRACKET";
  case TokenKind::ArrowKind:
    return "FUN_RETURN";
  case TokenKind::CommaKind:
    return "COMMA";
  case TokenKind::SemicolonKind:
    return "SEMICOLON";

  // Basic operators
  case TokenKind::PlusKind:
    return "ADD";
  case TokenKind::MinusKind:
    return "SUB";
  case TokenKind::StarKind:
    return "MUL";
  case TokenKind::SlashKind:
    return "DIV";
  case TokenKind::PercentKind:
    return "MOD";
  case TokenKind::BangKind:
    return "BANG";

  // Assignment operators
  case TokenKind::PlusEqualsKind:
    return "PLUS_EQUALS";
  case TokenKind::SubEqualsKind:
    return "SUB_EQUALS";
  case TokenKind::MulEqualKind:
    return "MUL_EQUALS";
  case TokenKind::DivEqualsKind:
    return "DIV_EQUALS";
  case TokenKind::ModEqualsKind:
    return "MOD_EQUALS";

  // Member access
  case TokenKind::PeriodKind:
    return "MEMBER";
  case TokenKind::DoubleColonKind:
    return "NAMESPACE_MEMBER";

  // Increment/decrement
  case TokenKind::DoublePlusKind:
    return "INCREMENT";
  case TokenKind::DoubleMinusKind:
    return "DECREMENT";

  // Comparison
  case TokenKind::DoubleEqualsKind:
    return "EQUAL";
  case TokenKind::BangEqualsKind:
    return "NOT_EQUAL";

  // Logical
  case TokenKind::DoubleAmpKind:
    return "AND";
  case TokenKind::DoublePipeKind:
    return "OR";

  // Relational
  case TokenKind::OpenCaretKind:
    return "LESS";
  case TokenKind::LessEqualKind:
    return "LESS_EQUAL";
  case TokenKind::CloseCaretKind:
    return "GREATER";
  case TokenKind::GreaterEqualKind:
    return "GREATER_EQUAL";

  // Other operators
  case TokenKind::EqualsKind:
    return "ASSIGN";
  case TokenKind::ColonKind:
    return "COLON";

  case TokenKind::ExclRangeKind:
    return "EXCLUSIVE_RANGE";
  case TokenKind::InclRangeKind:
    return "INCLUSIVE_RANGE";

  // Literals
  case TokenKind::IntLiteralKind:
    return "INT_LITERAL";
  case TokenKind::FloatLiteralKind:
    return "FLOAT_LITERAL";
  case TokenKind::StrLiteralKind:
    return "STRING_LITERAL";
  case TokenKind::CharLiteralKind:
    return "CHAR_LITERAL";

  // Identifier
  case TokenKind::IdentifierKind:
    return "IDENTIFIER";

  default:
    return "UNKNOWN";
  }
}

} // namespace phi
