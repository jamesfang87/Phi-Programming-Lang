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
  case TokenKind::Eof:
    return "EOF";
  case TokenKind::Error:
    return "ERROR";

  // Keywords
  case TokenKind::BoolKw:
    return "BOOL";
  case TokenKind::BreakKw:
    return "BREAK";
  case TokenKind::ConstKw:
    return "CONST";
  case TokenKind::ContinueKw:
    return "CONTINUE";
  case TokenKind::DeferKw:
    return "DEFER";
  case TokenKind::ElseKwKind:
    return "ELSE";
  case TokenKind::EnumKw:
    return "ENUM";
  case TokenKind::FalseKw:
    return "FALSE";
  case TokenKind::ForKw:
    return "FOR";
  case TokenKind::FunKw:
    return "FUN";
  case TokenKind::IfKw:
    return "IF";
  case TokenKind::ImportKw:
    return "IMPORT";
  case TokenKind::InKwKind:
    return "IN";
  case TokenKind::PublicKw:
    return "PUBLIC";
  case TokenKind::ReturnKw:
    return "RETURN";
  case TokenKind::StructKw:
    return "STRUCT";
  case TokenKind::TrueKw:
    return "TRUE";
  case TokenKind::ThisKw:
    return "THIS";
  case TokenKind::VarKw:
    return "VAR";
  case TokenKind::WhileKw:
    return "WHILE";

  // Signed integer types
  case TokenKind::I8:
    return "I8";
  case TokenKind::I16:
    return "I16";
  case TokenKind::I32:
    return "I32";
  case TokenKind::I64:
    return "I64";

  // Unsigned integer types
  case TokenKind::U8:
    return "U8";
  case TokenKind::U16:
    return "U16";
  case TokenKind::U32:
    return "U32";
  case TokenKind::U64:
    return "U64";

  // Floating point types
  case TokenKind::F32:
    return "F32";
  case TokenKind::F64:
    return "F64";

  // Text types
  case TokenKind::String:
    return "STRING";
  case TokenKind::Char:
    return "CHAR";

  // Syntax elements
  case TokenKind::OpenParen:
    return "OPEN_PAREN";
  case TokenKind::CloseParen:
    return "CLOSE_PAREN";
  case TokenKind::OpenBrace:
    return "OPEN_BRACE";
  case TokenKind::CloseBrace:
    return "CLOSE_BRACE";
  case TokenKind::OpenBracket:
    return "OPEN_BRACKET";
  case TokenKind::CloseBracket:
    return "CLOSE_BRACKET";
  case TokenKind::Arrow:
    return "FUN_RETURN";
  case TokenKind::Comma:
    return "COMMA";
  case TokenKind::Semicolon:
    return "SEMICOLON";

  // Basic operators
  case TokenKind::Plus:
    return "ADD";
  case TokenKind::Minus:
    return "SUB";
  case TokenKind::Star:
    return "MUL";
  case TokenKind::Slash:
    return "DIV";
  case TokenKind::Percent:
    return "MOD";
  case TokenKind::Bang:
    return "BANG";
  case TokenKind::Amp:
    return "AMP";

  // Assignment operators
  case TokenKind::PlusEquals:
    return "PLUS_EQUALS";
  case TokenKind::SubEquals:
    return "SUB_EQUALS";
  case TokenKind::MulEquals:
    return "MUL_EQUALS";
  case TokenKind::DivEquals:
    return "DIV_EQUALS";
  case TokenKind::ModEquals:
    return "MOD_EQUALS";

  // Member access
  case TokenKind::Period:
    return "MEMBER";
  case TokenKind::DoubleColon:
    return "NAMESPACE_MEMBER";

  // Increment/decrement
  case TokenKind::DoublePlus:
    return "INCREMENT";
  case TokenKind::DoubleMinus:
    return "DECREMENT";

  // Comparison
  case TokenKind::DoubleEquals:
    return "EQUAL";
  case TokenKind::BangEquals:
    return "NOT_EQUAL";

  // Logical
  case TokenKind::DoubleAmp:
    return "AND";
  case TokenKind::DoublePipe:
    return "OR";

  // Relational
  case TokenKind::OpenCaret:
    return "LESS";
  case TokenKind::LessEqual:
    return "LESS_EQUAL";
  case TokenKind::CloseCaret:
    return "GREATER";
  case TokenKind::GreaterEqual:
    return "GREATER_EQUAL";

  // Other operators
  case TokenKind::Equals:
    return "ASSIGN";
  case TokenKind::Colon:
    return "COLON";

  case TokenKind::ExclRange:
    return "EXCLUSIVE_RANGE";
  case TokenKind::InclRange:
    return "INCLUSIVE_RANGE";

  // Literals
  case TokenKind::IntLiteral:
    return "INT_LITERAL";
  case TokenKind::FloatLiteral:
    return "FLOAT_LITERAL";
  case TokenKind::StrLiteral:
    return "STRING_LITERAL";
  case TokenKind::CharLiteral:
    return "CHAR_LITERAL";

  // Identifier
  case TokenKind::Identifier:
    return "IDENTIFIER";

  default:
    return "UNKNOWN";
  }
}

} // namespace phi
