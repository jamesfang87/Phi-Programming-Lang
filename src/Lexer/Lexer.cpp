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
        lineNum++;
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
  return {ret, !diagnosticManager->has_errors()};
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
    return makeToken(TokenType::tokOpenParen);
  case ')':
    return makeToken(TokenType::tokRightParen);
  case '{':
    return makeToken(TokenType::tokLeftBrace);
  case '}':
    return makeToken(TokenType::tokRightBrace);
  case '[':
    return makeToken(TokenType::tokLeftBracket);
  case ']':
    return makeToken(TokenType::tokRightBracket);
  case ',':
    return makeToken(TokenType::tokComma);
  case ';':
    return makeToken(TokenType::tokSemicolon);

  case '.':
    if (matchNextN(".="))
      return makeToken(TokenType::tokInclusiveRange);
    if (matchNext('.'))
      return makeToken(TokenType::tokExclusiveRange);
    return makeToken(TokenType::tokPeriod);
  case ':':
    return makeToken(matchNext(':') ? TokenType::tokDoubleColon
                                    : TokenType::tokColon);

  // Operators
  case '+':
    if (matchNext('+'))
      return makeToken(TokenType::tokDoublePlus);
    if (matchNext('='))
      return makeToken(TokenType::tokPlusEquals);
    return makeToken(TokenType::tokPlus);
  case '-':
    if (matchNext('>'))
      return makeToken(TokenType::tokArrow);
    if (matchNext('-'))
      return makeToken(TokenType::tokDoubleMinus);
    if (matchNext('='))
      return makeToken(TokenType::tokSubEquals);
    return makeToken(TokenType::tokMinus);
  case '*':
    return makeToken(matchNext('=') ? TokenType::tokMulEquals
                                    : TokenType::tokStar);
  case '/':
    return makeToken(matchNext('=') ? TokenType::tokDivEquals
                                    : TokenType::tokSlash);
  case '%':
    return makeToken(matchNext('=') ? TokenType::tokModEquals
                                    : TokenType::tokPercent);
  case '!':
    return makeToken(matchNext('=') ? TokenType::tokBangEquals
                                    : TokenType::tokBang);
  case '=':
    return makeToken(matchNext('=') ? TokenType::tokDoubleEquals
                                    : TokenType::tokEquals);
  case '<':
    return makeToken(matchNext('=') ? TokenType::tokLessEqual
                                    : TokenType::tokLeftCaret);
  case '>':
    return makeToken(matchNext('=') ? TokenType::tokGreaterEqual
                                    : TokenType::tokRightCaret);
  // Handle single & as error or bitwise operator
  case '&':
    if (matchNext('&')) {
      return makeToken(TokenType::tokDoubleAmp);
    } else {
      error("unexpected character '&'")
          .with_primary_label(getCurSpan(), "unexpected character")
          .with_help("use '&&' for logical AND operation")
          .with_note("single '&' is not supported in this language")
          .emit(*diagnosticManager);
      return makeToken(TokenType::tokError);
    }
  // Handle single | as error or bitwise operator
  case '|':
    if (matchNext('|')) {
      return makeToken(TokenType::tokDoublePipe);
    } else {
      error("unexpected character '|'")
          .with_primary_label(getCurSpan(), "unexpected character")
          .with_help("use '||' for logical OR operation")
          .with_note("single '|' is not supported in this language")
          .emit(*diagnosticManager);
      return makeToken(TokenType::tokError);
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
    std::string charDisplay;
    if (std::isprint(c)) {
      charDisplay = "'" + std::string(1, c) + "'";
    } else {
      charDisplay =
          "\\x" + std::format("{:02x}", static_cast<unsigned char>(c));
    }

    error("unexpected character " + charDisplay)
        .with_primary_label(getCurSpan(), "unexpected character")
        .with_help("remove this character or use a valid token")
        .with_note("valid characters include letters, digits, operators, and "
                   "punctuation")
        .emit(*diagnosticManager);

    return makeToken(TokenType::tokError);
  }
  }
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

void Lexer::emitError(std::string_view msg, std::string_view helpMsg) {
  auto diagnostic = error(std::string(msg)).with_primary_label(getCurSpan());

  if (!helpMsg.empty()) {
    diagnostic.with_help(std::string(helpMsg));
  }

  diagnostic.emit(*diagnosticManager);
}

void Lexer::emitUnclosedBlockCommentError(std::string::iterator startPos,
                                          std::string::iterator startLine,
                                          int startLineNum) {
  // Calculate current position (where we reached EOF)
  int curCol = static_cast<int>(curChar - curLine) + 1;
  SrcLocation curStart{.path = path, .line = lineNum, .col = curCol};
  SrcLocation curEnd{.path = path, .line = lineNum, .col = curCol};
  SrcSpan curSpan{curStart, curEnd};

  // Calculate where the block comment started using passed parameters
  int col = static_cast<int>(startPos - startLine) + 1;
  SrcLocation start{.path = path, .line = startLineNum, .col = col};
  SrcLocation end{.path = path, .line = startLineNum, .col = col + 2};
  SrcSpan span{start, end};

  error("unclosed block comment")
      .with_primary_label(span, "block comment starts here")
      .with_help("add a closing `*/` to terminate the block comment")
      .emit(*diagnosticManager);
}

SrcLocation Lexer::getCurLocation() const {
  int col = static_cast<int>(curChar - lexemeLine) + 1;
  return SrcLocation{.path = path, .line = lineNum, .col = col};
}

SrcSpan Lexer::getCurSpan() const {
  int startCol = static_cast<int>(curLexeme - lexemeLine) + 1;
  int endCol = static_cast<int>(curChar - curLine) + 1;

  SrcLocation start{.path = path, .line = lineNum, .col = startCol};
  SrcLocation end{.path = path, .line = lineNum, .col = endCol};

  return SrcSpan{start, end};
}

} // namespace phi
