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
#include "Lexer/TokenKind.hpp"

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
std::vector<Token> Lexer::scan() {
  std::vector<Token> Tokens;
  while (!atEOF()) {
    // make sure that these two iters are pointing to the same place
    CurLexeme = CurChar;
    LexemeLine = CurLine;

    // handle whitespace
    if (std::isspace(peekChar())) {
      if (advanceChar() == '\n') {
        LineNum++;
        CurLine = CurChar;
      }
      continue;
    }

    // handle comments
    if (peekChar() == '/' && (peekNext() == '/' || peekNext() == '*')) {
      skipComment();
      continue;
    }

    // finally, scan the token
    Tokens.push_back(scanToken());
  }
  return Tokens;
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
  switch (char C = advanceChar()) {
  // One char tokens
  case '(':
    return makeToken(TokenKind::OpenParen);
  case ')':
    return makeToken(TokenKind::CloseParen);
  case '{':
    return makeToken(TokenKind::OpenBrace);
  case '}':
    return makeToken(TokenKind::CloseBrace);
  case '[':
    return makeToken(TokenKind::OpenBracket);
  case ']':
    return makeToken(TokenKind::CloseBracket);
  case ',':
    return makeToken(TokenKind::Comma);
  case ';':
    return makeToken(TokenKind::Semicolon);

  case '.':
    if (matchNextN(".="))
      return makeToken(TokenKind::InclRange);
    if (matchNext('.'))
      return makeToken(TokenKind::ExclRange);
    return makeToken(TokenKind::Period);
  case ':':
    return makeToken(matchNext(':') ? TokenKind::DoubleColon
                                    : TokenKind::Colon);

  // Operators
  case '+':
    if (matchNext('+'))
      return makeToken(TokenKind::DoublePlus);
    if (matchNext('='))
      return makeToken(TokenKind::PlusEquals);
    return makeToken(TokenKind::Plus);
  case '-':
    if (matchNext('>'))
      return makeToken(TokenKind::Arrow);
    if (matchNext('-'))
      return makeToken(TokenKind::DoubleMinus);
    if (matchNext('='))
      return makeToken(TokenKind::SubEquals);
    return makeToken(TokenKind::Minus);
  case '*':
    return makeToken(matchNext('=') ? TokenKind::MulEqual : TokenKind::Star);
  case '/':
    return makeToken(matchNext('=') ? TokenKind::DivEquals : TokenKind::Slash);
  case '%':
    return makeToken(matchNext('=') ? TokenKind::ModEquals
                                    : TokenKind::Percent);
  case '!':
    return makeToken(matchNext('=') ? TokenKind::BangEquals : TokenKind::Bang);
  case '=':
    return makeToken(matchNext('=') ? TokenKind::DoubleEquals
                                    : TokenKind::Equals);
  case '<':
    return makeToken(matchNext('=') ? TokenKind::LessEqual
                                    : TokenKind::OpenCaret);
  case '>':
    return makeToken(matchNext('=') ? TokenKind::GreaterEqual
                                    : TokenKind::CloseCaret);
  // Handle single & as error or bitwise operator
  case '&':
    if (matchNext('&')) {
      return makeToken(TokenKind::DoubleAmp);
    } else {
      return makeToken(TokenKind::Amp);
    }
  // Handle single | as error or bitwise operator
  case '|':
    if (matchNext('|')) {
      return makeToken(TokenKind::DoublePipe);
    } else {
      error("unexpected character '|'")
          .with_primary_label(getCurSpan(), "unexpected character")
          .with_help("use '||' for logical OR operation")
          .with_note("single '|' is not supported in this language")
          .emit(*DiagnosticsMan);
      return makeToken(TokenKind::Error);
    }

  case '"':
    return parseString();
  case '\'':
    return parseChar();

  default: {
    if (std::isalpha(C) || C == '_') {
      return parseIdentifierOrKw();
    }
    if (std::isdigit(C)) {
      return parseNumber();
    }

    // Handle unknown character with better error message
    std::string charDisplay;
    if (std::isprint(C)) {
      charDisplay = "'" + std::string(1, C) + "'";
    } else {
      charDisplay =
          "\\x" + std::format("{:02x}", static_cast<unsigned char>(C));
    }

    error("unexpected character " + charDisplay)
        .with_primary_label(getCurSpan(), "unexpected character")
        .with_help("remove this character or use a valid token")
        .with_note("valid characters include letters, digits, operators, and "
                   "punctuation")
        .emit(*DiagnosticsMan);

    return makeToken(TokenKind::Error);
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

  diagnostic.emit(*DiagnosticsMan);
}

void Lexer::emitUnclosedBlockCommentError(std::string::iterator startPos,
                                          std::string::iterator startLine,
                                          int startLineNum) {
  // Calculate current position (where we reached EOF)
  int curCol = static_cast<int>(CurChar - CurLine) + 1;
  SrcLocation curStart{.path = Path, .line = LineNum, .col = curCol};
  SrcLocation curEnd{.path = Path, .line = LineNum, .col = curCol};
  SrcSpan curSpan{curStart, curEnd};

  // Calculate where the block comment started using passed parameters
  int col = static_cast<int>(startPos - startLine) + 1;
  SrcLocation start{.path = Path, .line = startLineNum, .col = col};
  SrcLocation end{.path = Path, .line = startLineNum, .col = col + 2};
  SrcSpan span{start, end};

  error("unclosed block comment")
      .with_primary_label(span, "block comment starts here")
      .with_help("add a closing `*/` to terminate the block comment")
      .emit(*DiagnosticsMan);
}

SrcLocation Lexer::getCurLocation() const {
  int col = static_cast<int>(CurChar - LexemeLine) + 1;
  return SrcLocation{.path = Path, .line = LineNum, .col = col};
}

SrcSpan Lexer::getCurSpan() const {
  int startCol = static_cast<int>(CurLexeme - LexemeLine) + 1;
  int endCol = static_cast<int>(CurChar - CurLine) + 1;

  SrcLocation start{.path = Path, .line = LineNum, .col = startCol};
  SrcLocation end{.path = Path, .line = LineNum, .col = endCol};

  return SrcSpan{start, end};
}

} // namespace phi
