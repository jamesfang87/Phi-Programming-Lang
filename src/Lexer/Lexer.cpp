/**
 * @file Lexer.cpp
 * @brief Implementation of the Scanner class for lexical analysis
 *
 * This file contains the complete implementation of the Scanner class,
 * including all token parsing methods, error handling, and helper functions.
 * The scanner processes Phi source code character by character, producing a
 * stream of tokens that can be consumed by the parser.
 *
 * Key features implemented:
 * - Multi-character operator recognition (e.g., ==, +=, ->)
 * - String literal parsing with full escape sequence support
 * - Numeric literal parsing (integers and floats)
 * - Keyword vs identifier distinction
 * - Comment handling (line and block comments)
 * - Comprehensive error reporting using diagnostic system
 */

#include "Lexer/Lexer.hpp"

#include <cstring>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/Token.hpp"

namespace phi {

// =============================================================================
// PUBLIC INTERFACE IMPLEMENTATION
// =============================================================================

// =============================================================================
// MAIN SCANNING LOGIC
// =============================================================================

/**
 * @brief Main scanning loop that processes the entire source code
 *
 * This is the primary entry point for lexical analysis. It iterates through
 * the source code character by character, handling whitespace, comments, and
 * delegating token recognition to scan_token(). The method maintains proper
 * line and column tracking throughout the scanning process.
 *
 * The scanning process:
 * 1. Skip whitespace (tracking newlines for line numbers)
 * 2. Skip comments (both line comments and block comments)
 * 3. Scan individual tokens using scan_token()
 * 4. Continue until end of file
 *
 * @return A pair containing the vector of tokens and success status
 */
std::pair<std::vector<Token>, bool> Lexer::scan() {
  std::vector<Token> ret;

  while (!atEOF()) {
    // make sure that these two it are pointing to the same place
    curLexeme = curChar;
    lexemeLine = curLine;

    // handle whitespace
    if (std::isspace(peekChar())) {
      if (advanceChar() == '\n') {
        line_num++;
        curLine = curChar;
      }
      continue;
    }

    // handle comments
    if (peekChar() == '/' && (peekNext() == '/' || peekNext() == '*')) {
      skipComment();
      continue;
    }

    // finally, scan the token
    ret.push_back(scanToken());
  }
  return {ret, !diagnosticsManager->has_errors()};
}

/**
 * @brief Scans a single token from the current position
 *
 * This is the main token recognition method that analyzes the current character
 * and determines what type of token to create. It handles:
 *
 * Single-character tokens:
 * - Parentheses, braces, brackets
 * - Basic operators (+, -, *, /, %, !)
 * - Punctuation (comma, semicolon, colon, dot)
 *
 * Multi-character tokens:
 * - Compound operators (==, !=, <=, >=, ++, --, +=, -=, etc.)
 * - Function return arrow (->)
 * - Namespace member (::)
 * - Logical operators (&&, ||)
 *
 * Complex tokens (delegated to specialized methods):
 * - String literals (parse_string)
 * - Character literals (parse_char)
 * - Numeric literals (parse_number)
 * - Identifiers and keywords (parse_identifier)
 *
 * @return A token representing the scanned lexical element
 */
Token Lexer::scanToken() {
  switch (char c = advanceChar()) {
  // One char tokens
  case '(':
    return makeToken(TokenType::tok_open_paren);
  case ')':
    return makeToken(TokenType::tok_close_paren);
  case '{':
    return makeToken(TokenType::tok_open_brace);
  case '}':
    return makeToken(TokenType::tok_close_brace);
  case '[':
    return makeToken(TokenType::tok_open_bracket);
  case ']':
    return makeToken(TokenType::tok_close_bracket);
  case ',':
    return makeToken(TokenType::tok_comma);
  case ';':
    return makeToken(TokenType::tok_semicolon);

  case '.':
    if (matchNextN(".="))
      return makeToken(TokenType::tok_inclusive_range);
    if (matchNext('.'))
      return makeToken(TokenType::tok_exclusive_range);
    return makeToken(TokenType::tok_member);
  case ':':
    return makeToken(matchNext(':') ? TokenType::tok_namespace_member
                                    : TokenType::tok_colon);

  // Operators
  case '+':
    if (matchNext('+'))
      return makeToken(TokenType::tok_increment);
    if (matchNext('='))
      return makeToken(TokenType::tok_plus_equals);
    return makeToken(TokenType::tok_add);
  case '-':
    if (matchNext('>'))
      return makeToken(TokenType::tok_fun_return);
    if (matchNext('-'))
      return makeToken(TokenType::tok_decrement);
    if (matchNext('='))
      return makeToken(TokenType::tok_sub_equals);
    return makeToken(TokenType::tok_sub);
  case '*':
    return makeToken(matchNext('=') ? TokenType::tok_mul_equals
                                    : TokenType::tok_mul);
  case '/':
    return makeToken(matchNext('=') ? TokenType::tok_div_equals
                                    : TokenType::tok_div);
  case '%':
    return makeToken(matchNext('=') ? TokenType::tok_mod_equals
                                    : TokenType::tok_mod);
  case '!':
    return makeToken(matchNext('=') ? TokenType::tok_not_equal
                                    : TokenType::tok_bang);
  case '=':
    return makeToken(matchNext('=') ? TokenType::tok_equal
                                    : TokenType::tok_assign);
  case '<':
    return makeToken(matchNext('=') ? TokenType::tok_less_equal
                                    : TokenType::tok_less);
  case '>':
    return makeToken(matchNext('=') ? TokenType::tok_greater_equal
                                    : TokenType::tok_greater);
  // Handle single & as error or bitwise operator
  case '&':
    if (matchNext('&')) {
      return makeToken(TokenType::tok_and);
    } else {
      error("unexpected character '&'")
          .with_primary_label(getCurSpan(), "unexpected character")
          .with_help("use '&&' for logical AND operation")
          .with_note("single '&' is not supported in this language")
          .emit(*diagnosticsManager);
      return makeToken(TokenType::tok_error);
    }
  // Handle single | as error or bitwise operator
  case '|':
    if (matchNext('|')) {
      return makeToken(TokenType::tok_or);
    } else {
      error("unexpected character '|'")
          .with_primary_label(getCurSpan(), "unexpected character")
          .with_help("use '||' for logical OR operation")
          .with_note("single '|' is not supported in this language")
          .emit(*diagnosticsManager);
      return makeToken(TokenType::tok_error);
    }

  case '"':
    return parseStr();
  case '\'':
    return parseChar();

  default: {
    if (std::isalpha(c) || c == '_') {
      return parseIdentifierOrKw();
    }
    if (std::isdigit(c)) {
      return parseNumber();
    }

    // Handle unknown character with better error message
    std::string char_display;
    if (std::isprint(c)) {
      char_display = "'" + std::string(1, c) + "'";
    } else {
      char_display =
          "\\x" + std::format("{:02x}", static_cast<unsigned char>(c));
    }

    error("unexpected character " + char_display)
        .with_primary_label(getCurSpan(), "unexpected character")
        .with_help("remove this character or use a valid token")
        .with_note("valid characters include letters, digits, operators, and "
                   "punctuation")
        .emit(*diagnosticsManager);

    return makeToken(TokenType::tok_error);
  }
  }
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

void Lexer::emitError(std::string_view message, std::string_view help_message) {
  auto diagnostic =
      error(std::string(message)).with_primary_label(getCurSpan());

  if (!help_message.empty()) {
    diagnostic.with_help(std::string(help_message));
  }

  diagnostic.emit(*diagnosticsManager);
}

void Lexer::emitUnclosedBlockCommentError(
    std::string::iterator comment_start_pos,
    std::string::iterator comment_start_line, int comment_start_line_num) {
  // Calculate current position (where we reached EOF)
  int current_col = static_cast<int>(curChar - curLine) + 1;
  SrcLocation current_start{.path = path, .line = line_num, .col = current_col};
  SrcLocation current_end{.path = path, .line = line_num, .col = current_col};
  SrcSpan current_span{current_start, current_end};

  // Calculate where the block comment started using passed parameters
  int comment_col =
      static_cast<int>(comment_start_pos - comment_start_line) + 1;
  SrcLocation start_loc{
      .path = path, .line = comment_start_line_num, .col = comment_col};
  SrcLocation end_loc{
      .path = path, .line = comment_start_line_num, .col = comment_col + 2};
  SrcSpan comment_span{start_loc, end_loc};

  error("unclosed block comment")
      .with_primary_label(comment_span, "block comment starts here")
      .with_help("add a closing `*/` to terminate the block comment")
      .emit(*diagnosticsManager);
}

SrcLocation Lexer::getCurLocation() const {
  int col = static_cast<int>(curChar - lexemeLine) + 1;
  return SrcLocation{.path = path, .line = line_num, .col = col};
}

SrcSpan Lexer::getCurSpan() const {
  int start_col = static_cast<int>(curLexeme - lexemeLine) + 1;
  int end_col = static_cast<int>(curChar - curLine) + 1;

  SrcLocation start{.path = path, .line = line_num, .col = start_col};
  SrcLocation end{.path = path, .line = line_num, .col = end_col};

  return SrcSpan{start, end};
}

} // namespace phi
