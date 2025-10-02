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
    return makeToken(matchNext('=') ? TokenKind::MulEquals : TokenKind::Star);
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
    std::string CharDisplay;
    if (std::isprint(C)) {
      CharDisplay = "'" + std::string(1, C) + "'";
    } else {
      CharDisplay =
          "\\x" + std::format("{:02x}", static_cast<unsigned char>(C));
    }

    error("unexpected character " + CharDisplay)
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
  auto Diag = error(std::string(msg)).with_primary_label(getCurSpan());

  if (!helpMsg.empty()) {
    Diag.with_help(std::string(helpMsg));
  }

  Diag.emit(*DiagnosticsMan);
}

void Lexer::emitUnclosedBlockCommentError(std::string::iterator StartPos,
                                          std::string::iterator StartLine,
                                          int StartLineNum) {
  // Calculate current position (where we reached EOF)
  int CurCol = static_cast<int>(CurChar - CurLine) + 1;
  SrcLocation CurStart{.Path = Path, .Line = LineNum, .Col = CurCol};
  SrcLocation CurEnd{.Path = Path, .Line = LineNum, .Col = CurCol};
  SrcSpan CurSpan{CurStart, CurEnd};

  // Calculate where the block comment started using passed parameters
  int Col = static_cast<int>(StartPos - StartLine) + 1;
  SrcLocation Start{.Path = Path, .Line = StartLineNum, .Col = Col};
  SrcLocation End{.Path = Path, .Line = StartLineNum, .Col = Col + 2};
  SrcSpan Span{Start, End};

  error("unclosed block comment")
      .with_primary_label(Span, "block comment starts here")
      .with_help("add a closing `*/` to terminate the block comment")
      .emit(*DiagnosticsMan);
}

SrcLocation Lexer::getCurLocation() const {
  int Col = static_cast<int>(CurChar - LexemeLine) + 1;
  return SrcLocation{.Path = Path, .Line = LineNum, .Col = Col};
}

SrcSpan Lexer::getCurSpan() const {
  int StartCol = static_cast<int>(CurLexeme - LexemeLine) + 1;
  int EndCol = static_cast<int>(CurChar - CurLine) + 1;

  SrcLocation Start{.Path = Path, .Line = LineNum, .Col = StartCol};
  SrcLocation End{.Path = Path, .Line = LineNum, .Col = EndCol};

  return SrcSpan{Start, End};
}

} // namespace phi
