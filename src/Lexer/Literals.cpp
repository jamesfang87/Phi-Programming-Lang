#include "Lexer/Lexer.hpp"

#include <regex>
#include <unordered_map>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenType.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Parses numeric literals (integers and floating-point numbers)
 *
 * This method handles the parsing of numeric literals, automatically detecting
 * whether the number is an integer or floating-point based on the presence of
 * a decimal point. The method supports:
 * - Integer literals: 42, 123, 0
 * - Floating-point literals: 3.14, 0.5, 123.456
 *
 * @note Exponential notation (e.g., 1.23e4) is planned but not yet implemented
 * @return A token of type tok_int_literal or tok_float_literal
 */
Token Lexer::parseNumber() {
  while (std::isdigit(peekChar())) {
    advanceChar();
  }

  // fractional part, only if the next two chars match .[0-9]
  std::regex pattern("^.[0-9]$");
  if (std::regex_match(peekNextN(2), pattern)) {
    advanceChar(); // consume '.'
    while (std::isdigit(peekChar())) {
      advanceChar();
    }
  } else {
    return makeToken(TokenKind::tokIntLiteral);
  }

  // TODO: implement exponents
  return makeToken(TokenKind::tokFloatLiteral);
}

/**
 * @brief Parses identifiers and distinguishes them from keywords
 *
 * This method parses sequences of alphanumeric characters and underscores
 * that start with a letter or underscore. It then checks the parsed identifier
 * against the keyword table to determine if it should be tokenized
 * as a keyword or as a user-defined identifier.
 *
 * The keyword table includes:
 * - Control flow keywords (if, else, for, while, etc.)
 * - Declaration keywords (let, fun, class, etc.)
 * - Type keywords (i32, str, bool, etc.)
 * - Literal keywords (true, false)
 *
 * @return A token of the appropriate keyword type or tok_identifier
 */
Token Lexer::parseIdentifierOrKw() {
  while (std::isalnum(peekChar()) || peekChar() == '_') {
    advanceChar();
  }
  const std::string id(curLexeme, curChar);

  static const std::unordered_map<std::string, TokenKind> keywords = {
      {"bool", TokenKind::tokBool},     {"break", TokenKind::tokBreak},
      {"const", TokenKind::tokConst},   {"continue", TokenKind::tokContinue},
      {"else", TokenKind::tokElse},     {"enum", TokenKind::TokEnum},
      {"false", TokenKind::tokFalse},   {"for", TokenKind::tokFor},
      {"fun", TokenKind::tokFun},       {"if", TokenKind::tokIf},
      {"import", TokenKind::tokImport}, {"in", TokenKind::tokIn},
      {"let", TokenKind::tokLet},       {"return", TokenKind::tokReturn},
      {"struct", TokenKind::TokStruct}, {"true", TokenKind::tokTrue},
      {"while", TokenKind::tokWhile},   {"i8", TokenKind::tokI8},
      {"i16", TokenKind::tokI16},       {"i32", TokenKind::tokI32},
      {"i64", TokenKind::tokI64},       {"u8", TokenKind::tokU8},
      {"u16", TokenKind::tokU16},       {"u32", TokenKind::tokU32},
      {"u64", TokenKind::tokU64},       {"f32", TokenKind::tokF32},
      {"f64", TokenKind::tokF64},       {"string", TokenKind::tokString},
      {"char", TokenKind::tokChar},
  };

  const auto it = keywords.find(id);
  return it != keywords.end() ? makeToken(it->second)
                              : makeToken(TokenKind::tokIdentifier);
}

/**
 * @brief Parses string literals with escape sequence support
 *
 * This method parses a complete string literal enclosed in double quotes.
 * It handles escape sequences, multi-line strings, and proper error reporting
 * for unterminated strings. The method tracks line numbers correctly when
 * strings span multiple lines.
 *
 * Features:
 * - Full escape sequence support via parse_escape_sequence()
 * - Multi-line string support
 * - Proper line number tracking
 * - Error reporting for unterminated strings
 *
 * @return A token of type tok_str_literal containing the parsed string content
 */
Token Lexer::parseStr() {
  insideStr = true;
  const int startLineNum = lineNum;
  auto startPos = curLexeme;
  auto startLinePos = lexemeLine;
  std::string str;

  // parse until we see closing double quote
  while (insideStr && !atEOF() && peekChar() != '"') {
    if (peekChar() == '\\') {
      advanceChar(); // consume the forward slash
      str.push_back(parseEscapeSeq());
      continue;
    }
    // increment line number if we see '\n'
    if (peekChar() == '\n') {
      lineNum++;
      curLine = curChar;
      advanceChar();
    } else {
      str.push_back(advanceChar());
    }
  }

  if (atEOF()) {
    // reached eof without finding closing double quote
    int col = static_cast<int>(startPos - startLinePos) + 1;
    SrcLocation start{.path = path, .line = startLineNum, .col = col};
    SrcSpan span{start};
    error("unterminated string literal")
        .with_primary_label(span, "string starts here")
        .with_help("add a closing double quote (\") to terminate the string")
        .emit(*diagnosticManager);
    return makeToken(TokenKind::tokError);
  }
  advanceChar();     // consume closing quote
  insideStr = false; // we are no longer inside str

  SrcLocation start = {.path = this->path,
                       .line = startLineNum,
                       .col = static_cast<int>(curLexeme - lexemeLine) + 1};

  SrcLocation end = {.path = this->path,
                     .line = lineNum,
                     .col = static_cast<int>(curChar - curLine) + 1};

  return {start, end, TokenKind::tokStrLiteral, str};
}

/**
 * @brief Parses character literals with escape sequence support
 *
 * This method parses a complete character literal enclosed in single quotes.
 * It supports both regular characters and escape sequences. The character
 * literal must contain exactly one character (after escape sequence
 * processing).
 *
 * Examples:
 * - 'a' -> character 'a'
 * - '\n' -> newline character
 * - '\x41' -> character 'A'
 *
 * @return A token of type tok_char_literal containing the character value
 * @throws Scanning error for unterminated character literals
 */
Token Lexer::parseChar() {
  // handle case that char literal is empty
  if (peekChar() == '\'') {
    advanceChar(); // consume the opening quote
    advanceChar(); // consume the closing quote
    error("empty character literal")
        .with_primary_label(getCurSpan(), "character literal is empty")
        .with_help("character literals must contain exactly one character")
        .with_note(
            "try using a space character: ' ' or an escape sequence like '\\n'")
        .emit(*diagnosticManager);
    return makeToken(TokenKind::tokError);
  }

  char c;
  if (peekChar() != '\\') {
    c = advanceChar();
  } else {
    advanceChar(); // consume the forward slash
    c = parseEscapeSeq();
  }

  if (peekChar() != '\'') {
    if (atEOF() || peekChar() == '\n' || peekChar() == ';') {
      int col = static_cast<int>(curLexeme - lexemeLine) + 1;
      SrcLocation start{.path = path, .line = lineNum, .col = col};
      SrcSpan span{start};

      error("unterminated character literal")
          .with_primary_label(span, "character started here")
          .with_help(
              "add a closing single quote (') to terminate the character")
          .emit(*diagnosticManager);
    } else {
      error("character literal contains too many characters")
          .with_primary_label(getCurSpan(), "too many characters")
          .with_help("character literals must contain exactly one character")
          .with_note("use a string literal (\"\") for multiple characters")
          .emit(*diagnosticManager);
    }
    return makeToken(TokenKind::tokError);
  }

  advanceChar(); // consume closing quote

  SrcLocation start = {.path = this->path,
                       .line = lineNum,
                       .col = static_cast<int>(curLexeme - lexemeLine) + 1};

  SrcLocation end = {.path = this->path,
                     .line = lineNum,
                     .col = static_cast<int>(curChar - curLine) + 1};

  return {start, end, TokenKind::tokCharLiteral, std::string(1, c)};
}

/**
 * @brief Parses escape sequences within string and character literals
 *
 * This method handles the parsing of escape sequences that begin with a
 * backslash. Supported escape sequences include:
 * - \' (single quote)
 * - \" (double quote)
 * - \n (newline)
 * - \t (tab)
 * - \r (carriage return)
 * - \\\ (backslash)
 * - \0 (null character)
 * - \xNN (hexadecimal escape)
 *
 * @return The actual character value represented by the escape sequence
 * @throws Scanning error for invalid escape sequences
 */
char Lexer::parseEscapeSeq() {
  if (atEOF()) {
    error("unfinished escape sequence")
        .with_primary_label(getCurSpan(), "escape sequence incomplete")
        .with_help("add a valid escape character after the backslash")
        .with_note("valid escape sequences: \\n, \\t, \\r, \\\\, \\\", \\', "
                   "\\0, \\xNN")
        .emit(*diagnosticManager);
    return '\0';
  }

  // save location for error reporting
  SrcLocation loc = getCurLocation();
  switch (const char c = advanceChar()) {
  case '\'':
    return '\'';
  case '"':
    return '\"';
  case 'n':
    return '\n';
  case 't':
    return '\t';
  case 'r':
    return '\r';
  case '\\':
    return '\\';
  case '0':
    return '\0';
  case 'x':
    return parseHexEscape();
  default:
    error("unknown escape sequence")
        .with_primary_label(
            SrcSpan(loc),
            std::format("invalid char for escape sequence '\\{}'", c))
        .with_help("use a valid escape sequence")
        .with_note("valid escape sequences: \\n, \\t, \\r, \\\\, \\\", \\', "
                   "\\0, \\xNN")
        .emit(*diagnosticManager);
    return c; // Return character as-is for error recovery
  }
}

/**
 * @brief Parses hexadecimal escape sequences (\xNN format)
 *
 * This method parses exactly two hexadecimal digits following \x and converts
 * them to the corresponding character value. For example, \x41 becomes 'A'.
 *
 * @return The character value represented by the hex digits
 * @throws Scanning error if EOF is reached or invalid hex digits are found
 */
char Lexer::parseHexEscape() {
  if (!isxdigit(peekChar()) || !isxdigit(peekNext())) {
    error("incomplete hexadecimal escape sequence")
        .with_primary_label(getCurSpan(), "expected two hex digits here")
        .with_help(
            "hexadecimal escapes require exactly two digits: \\x00 to \\xFF")
        .with_note("example: \\x41 represents the character 'A'")
        .emit(*diagnosticManager);
    return '\0';
  }

  char hex[3] = {0};
  for (int i = 0; i < 2; i++) {
    if (atEOF() || !isxdigit(peekChar())) {
      error("incomplete hexadecimal escape sequence")
          .with_primary_label(getCurSpan(), "expected hex digit here")
          .with_help("hexadecimal escapes require exactly two digits")
          .emit(*diagnosticManager);
      return '\0';
    }
    hex[i] = advanceChar();
  }
  return static_cast<char>(std::stoi(hex, nullptr, 16));
}

} // namespace phi
