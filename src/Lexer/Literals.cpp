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
    return makeToken(TokenType::tok_int_literal);
  }

  // TODO: implement exponents
  return makeToken(TokenType::tok_float_literal);
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
  const std::string identifier(curLexeme, curChar);

  static const std::unordered_map<std::string, TokenType> keywords = {
      {"bool", TokenType::tok_bool},
      {"break", TokenType::tok_break},
      {"class", TokenType::tok_class},
      {"const", TokenType::tok_const},
      {"continue", TokenType::tok_continue},
      {"else", TokenType::tok_else},
      {"false", TokenType::tok_false},
      {"for", TokenType::tok_for},
      {"fun", TokenType::tok_fun},
      {"if", TokenType::tok_if},
      {"import", TokenType::tok_import},
      {"in", TokenType::tok_in},
      {"let", TokenType::tok_let},
      {"return", TokenType::tok_return},
      {"true", TokenType::tok_true},
      {"while", TokenType::tok_while},
      {"i8", TokenType::tok_i8},
      {"i16", TokenType::tok_i16},
      {"i32", TokenType::tok_i32},
      {"i64", TokenType::tok_i64},
      {"u8", TokenType::tok_u8},
      {"u16", TokenType::tok_u16},
      {"u32", TokenType::tok_u32},
      {"u64", TokenType::tok_u64},
      {"f32", TokenType::tok_f32},
      {"f64", TokenType::tok_f64},
      {"str", TokenType::tok_str},
      {"char", TokenType::tok_char}};

  const auto it = keywords.find(identifier);
  return it != keywords.end() ? makeToken(it->second)
                              : makeToken(TokenType::tok_identifier);
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
  const int starting_line = line_num;
  auto str_start_pos = curLexeme;
  auto str_start_line = lexemeLine;
  int str_start_line_num = line_num;
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
      line_num++;
      curLine = curChar;
      advanceChar();
    } else {
      str.push_back(advanceChar());
    }
  }

  if (atEOF()) {
    // reached eof without finding closing double quote
    int quote_col = static_cast<int>(str_start_pos - str_start_line) + 1;
    SrcLocation quote_start{
        .path = path, .line = str_start_line_num, .col = quote_col};
    SrcSpan quote_span{quote_start};
    error("unterminated string literal")
        .with_primary_label(quote_span, "string starts here")
        .with_help("add a closing double quote (\") to terminate the string")
        .emit(*diagnosticsManager);
    return makeToken(TokenType::tok_error);
  }
  advanceChar();     // consume closing quote
  insideStr = false; // we are no longer inside str

  SrcLocation start = {.path = this->path,
                       .line = starting_line,
                       .col = static_cast<int>(curLexeme - lexemeLine) + 1};

  SrcLocation end = {.path = this->path,
                     .line = line_num,
                     .col = static_cast<int>(curChar - curLine) + 1};

  return {start, end, TokenType::tok_str_literal, str};
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
        .emit(*diagnosticsManager);
    return makeToken(TokenType::tok_error);
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
      int quote_col = static_cast<int>(curLexeme - lexemeLine) + 1;
      SrcLocation quote_start{.path = path, .line = line_num, .col = quote_col};
      SrcSpan quote_span{quote_start};

      error("unterminated character literal")
          .with_primary_label(quote_span, "character started here")
          .with_help(
              "add a closing single quote (') to terminate the character")
          .emit(*diagnosticsManager);
    } else {
      error("character literal contains too many characters")
          .with_primary_label(getCurSpan(), "too many characters")
          .with_help("character literals must contain exactly one character")
          .with_note("use a string literal (\"\") for multiple characters")
          .emit(*diagnosticsManager);
    }
    return makeToken(TokenType::tok_error);
  }

  advanceChar(); // consume closing quote

  SrcLocation start = {.path = this->path,
                       .line = line_num,
                       .col = static_cast<int>(curLexeme - lexemeLine) + 1};

  SrcLocation end = {.path = this->path,
                     .line = line_num,
                     .col = static_cast<int>(curChar - curLine) + 1};

  return {start, end, TokenType::tok_char_literal, std::string(1, c)};
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
        .emit(*diagnosticsManager);
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
        .emit(*diagnosticsManager);
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
        .emit(*diagnosticsManager);
    return '\0';
  }

  char hex[3] = {0};
  for (int i = 0; i < 2; i++) {
    if (atEOF() || !isxdigit(peekChar())) {
      error("incomplete hexadecimal escape sequence")
          .with_primary_label(getCurSpan(), "expected hex digit here")
          .with_help("hexadecimal escapes require exactly two digits")
          .emit(*diagnosticsManager);
      return '\0';
    }
    hex[i] = advanceChar();
  }
  return static_cast<char>(std::stoi(hex, nullptr, 16));
}

} // namespace phi
