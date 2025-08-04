#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * @brief Converts a TokenType enumeration value to its string representation
 *
 * This function provides a mapping from TokenType enum values to their
 * corresponding human-readable string names. This is primarily used for
 * debugging output and error messages. The returned strings are in uppercase
 * and match the token type names without the "tok_" prefix.
 *
 * @param type The TokenType to convert
 * @return A string representation of the token type (e.g., "IDENTIFIER", "ADD")
 */

std::string tyToStr(const TokenType type) {
  switch (type) {
  // Special tokens
  case TokenType::tokEOF:
    return "EOF";
  case TokenType::tokError:
    return "ERROR";

  // Keywords
  case TokenType::tokBool:
    return "BOOL";
  case TokenType::tokBreak:
    return "BREAK";
  case TokenType::tokClass:
    return "CLASS";
  case TokenType::tokConst:
    return "CONST";
  case TokenType::tokContinue:
    return "CONTINUE";
  case TokenType::tokElse:
    return "ELSE";
  case TokenType::tokFalse:
    return "FALSE";
  case TokenType::tokFor:
    return "FOR";
  case TokenType::tokFun:
    return "FUN";
  case TokenType::tokIf:
    return "IF";
  case TokenType::tokImport:
    return "IMPORT";
  case TokenType::tokIn:
    return "IN";
  case TokenType::tokLet:
    return "LET";
  case TokenType::tokReturn:
    return "RETURN";
  case TokenType::tokTrue:
    return "TRUE";
  case TokenType::tokWhile:
    return "WHILE";

  // Signed integer types
  case TokenType::tokI8:
    return "I8";
  case TokenType::tokI16:
    return "I16";
  case TokenType::tokI32:
    return "I32";
  case TokenType::tokI64:
    return "I64";

  // Unsigned integer types
  case TokenType::tokU8:
    return "U8";
  case TokenType::tokU16:
    return "U16";
  case TokenType::tokU32:
    return "U32";
  case TokenType::tokU64:
    return "U64";

  // Floating point types
  case TokenType::tokF32:
    return "F32";
  case TokenType::tokF64:
    return "F64";

  // Text types
  case TokenType::tokStr:
    return "STR";
  case TokenType::tokChar:
    return "CHAR";

  // Syntax elements
  case TokenType::tokOpenParen:
    return "OPEN_PAREN";
  case TokenType::tokRightParen:
    return "CLOSE_PAREN";
  case TokenType::tokLeftBrace:
    return "OPEN_BRACE";
  case TokenType::tokRightBrace:
    return "CLOSE_BRACE";
  case TokenType::tokLeftBracket:
    return "OPEN_BRACKET";
  case TokenType::tokRightBracket:
    return "CLOSE_BRACKET";
  case TokenType::tokArrow:
    return "FUN_RETURN";
  case TokenType::tokComma:
    return "COMMA";
  case TokenType::tokSemicolon:
    return "SEMICOLON";

  // Basic operators
  case TokenType::tokPlus:
    return "ADD";
  case TokenType::tokMinus:
    return "SUB";
  case TokenType::tokStar:
    return "MUL";
  case TokenType::tokSlash:
    return "DIV";
  case TokenType::tokPercent:
    return "MOD";
  case TokenType::tokBang:
    return "BANG";

  // Assignment operators
  case TokenType::tokPlusEquals:
    return "PLUS_EQUALS";
  case TokenType::tokSubEquals:
    return "SUB_EQUALS";
  case TokenType::tokMulEquals:
    return "MUL_EQUALS";
  case TokenType::tokDivEquals:
    return "DIV_EQUALS";
  case TokenType::tokModEquals:
    return "MOD_EQUALS";

  // Member access
  case TokenType::tokPeriod:
    return "MEMBER";
  case TokenType::tokDoubleColon:
    return "NAMESPACE_MEMBER";

  // Increment/decrement
  case TokenType::tokDoublePlus:
    return "INCREMENT";
  case TokenType::tokDoubleMinus:
    return "DECREMENT";

  // Comparison
  case TokenType::tokDoubleEquals:
    return "EQUAL";
  case TokenType::tokBangEquals:
    return "NOT_EQUAL";

  // Logical
  case TokenType::tokDoubleAmp:
    return "AND";
  case TokenType::tokDoublePipe:
    return "OR";

  // Relational
  case TokenType::tokLeftCaret:
    return "LESS";
  case TokenType::tokLessEqual:
    return "LESS_EQUAL";
  case TokenType::tokRightCaret:
    return "GREATER";
  case TokenType::tokGreaterEqual:
    return "GREATER_EQUAL";

  // Other operators
  case TokenType::tokEquals:
    return "ASSIGN";
  case TokenType::tokColon:
    return "COLON";

  case TokenType::tokExclusiveRange:
    return "EXCLUSIVE_RANGE";
  case TokenType::tokInclusiveRange:
    return "INCLUSIVE_RANGE";

  // Literals
  case TokenType::tokIntLiteral:
    return "INT_LITERAL";
  case TokenType::tokFloatLiteral:
    return "FLOAT_LITERAL";
  case TokenType::tokStrLiteral:
    return "STRING_LITERAL";
  case TokenType::tokCharLiteral:
    return "CHAR_LITERAL";

  // Identifier
  case TokenType::tokIdentifier:
    return "IDENTIFIER";

  default:
    return "UNKNOWN";
  }
}

} // namespace phi
