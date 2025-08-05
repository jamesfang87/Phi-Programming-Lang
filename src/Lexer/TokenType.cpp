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

std::string tyToStr(const TokenKind type) {
  switch (type) {
  // Special tokens
  case TokenKind::tokEOF:
    return "EOF";
  case TokenKind::tokError:
    return "ERROR";

  // Keywords
  case TokenKind::tokBool:
    return "BOOL";
  case TokenKind::tokBreak:
    return "BREAK";
  case TokenKind::tokConst:
    return "CONST";
  case TokenKind::tokContinue:
    return "CONTINUE";
  case TokenKind::tokElse:
    return "ELSE";
  case TokenKind::TokEnum:
    return "ENUM";
  case TokenKind::tokFalse:
    return "FALSE";
  case TokenKind::tokFor:
    return "FOR";
  case TokenKind::tokFun:
    return "FUN";
  case TokenKind::tokIf:
    return "IF";
  case TokenKind::tokImport:
    return "IMPORT";
  case TokenKind::tokIn:
    return "IN";
  case TokenKind::tokLet:
    return "LET";
  case TokenKind::tokReturn:
    return "RETURN";
  case TokenKind::TokStruct:
    return "STRUCT";
  case TokenKind::tokTrue:
    return "TRUE";
  case TokenKind::tokWhile:
    return "WHILE";

  // Signed integer types
  case TokenKind::tokI8:
    return "I8";
  case TokenKind::tokI16:
    return "I16";
  case TokenKind::tokI32:
    return "I32";
  case TokenKind::tokI64:
    return "I64";

  // Unsigned integer types
  case TokenKind::tokU8:
    return "U8";
  case TokenKind::tokU16:
    return "U16";
  case TokenKind::tokU32:
    return "U32";
  case TokenKind::tokU64:
    return "U64";

  // Floating point types
  case TokenKind::tokF32:
    return "F32";
  case TokenKind::tokF64:
    return "F64";

  // Text types
  case TokenKind::tokString:
    return "STR";
  case TokenKind::tokChar:
    return "CHAR";

  // Syntax elements
  case TokenKind::tokOpenParen:
    return "OPEN_PAREN";
  case TokenKind::tokRightParen:
    return "CLOSE_PAREN";
  case TokenKind::tokLeftBrace:
    return "OPEN_BRACE";
  case TokenKind::tokRightBrace:
    return "CLOSE_BRACE";
  case TokenKind::tokLeftBracket:
    return "OPEN_BRACKET";
  case TokenKind::tokRightBracket:
    return "CLOSE_BRACKET";
  case TokenKind::tokArrow:
    return "FUN_RETURN";
  case TokenKind::tokComma:
    return "COMMA";
  case TokenKind::tokSemicolon:
    return "SEMICOLON";

  // Basic operators
  case TokenKind::tokPlus:
    return "ADD";
  case TokenKind::tokMinus:
    return "SUB";
  case TokenKind::tokStar:
    return "MUL";
  case TokenKind::tokSlash:
    return "DIV";
  case TokenKind::tokPercent:
    return "MOD";
  case TokenKind::tokBang:
    return "BANG";

  // Assignment operators
  case TokenKind::tokPlusEquals:
    return "PLUS_EQUALS";
  case TokenKind::tokSubEquals:
    return "SUB_EQUALS";
  case TokenKind::tokMulEquals:
    return "MUL_EQUALS";
  case TokenKind::tokDivEquals:
    return "DIV_EQUALS";
  case TokenKind::tokModEquals:
    return "MOD_EQUALS";

  // Member access
  case TokenKind::tokPeriod:
    return "MEMBER";
  case TokenKind::tokDoubleColon:
    return "NAMESPACE_MEMBER";

  // Increment/decrement
  case TokenKind::tokDoublePlus:
    return "INCREMENT";
  case TokenKind::tokDoubleMinus:
    return "DECREMENT";

  // Comparison
  case TokenKind::tokDoubleEquals:
    return "EQUAL";
  case TokenKind::tokBangEquals:
    return "NOT_EQUAL";

  // Logical
  case TokenKind::tokDoubleAmp:
    return "AND";
  case TokenKind::tokDoublePipe:
    return "OR";

  // Relational
  case TokenKind::tokLeftCaret:
    return "LESS";
  case TokenKind::tokLessEqual:
    return "LESS_EQUAL";
  case TokenKind::tokRightCaret:
    return "GREATER";
  case TokenKind::tokGreaterEqual:
    return "GREATER_EQUAL";

  // Other operators
  case TokenKind::tokEquals:
    return "ASSIGN";
  case TokenKind::tokColon:
    return "COLON";

  case TokenKind::tokExclusiveRange:
    return "EXCLUSIVE_RANGE";
  case TokenKind::tokInclusiveRange:
    return "INCLUSIVE_RANGE";

  // Literals
  case TokenKind::tokIntLiteral:
    return "INT_LITERAL";
  case TokenKind::tokFloatLiteral:
    return "FLOAT_LITERAL";
  case TokenKind::tokStrLiteral:
    return "STRING_LITERAL";
  case TokenKind::tokCharLiteral:
    return "CHAR_LITERAL";

  // Identifier
  case TokenKind::tokIdentifier:
    return "IDENTIFIER";

  default:
    return "UNKNOWN";
  }
}

} // namespace phi
