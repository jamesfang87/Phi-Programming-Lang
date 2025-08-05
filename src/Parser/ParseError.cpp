#include "Lexer/TokenType.hpp"
#include "Parser/Parser.hpp"

namespace phi {

/**
 * Emits an "expected X found Y" error message.
 *
 * @param expected Description of the expected token or syntax element
 * @param found_token The actual token encountered in the input stream
 *
 * Constructs a detailed error message showing what was expected versus what was
 * found, and highlights the location of the unexpected token in the source
 * code.
 */
void Parser::emitExpectedFoundError(const std::string &expected,
                                    const Token &foundToken) {
  error(
      std::format("expected {}, found `{}`", expected, foundToken.getLexeme()))
      .with_primary_label(spanFromToken(foundToken),
                          std::format("expected {} here", expected))
      .emit(*diagnosticManager);
}

/**
 * Emits an "unexpected token" error with suggestions for expected tokens.
 *
 * @param token The unexpected token encountered
 * @param expected_tokens List of valid token types expected at this position
 *
 * Generates an error message showing the unexpected token and provides a help
 * message listing valid alternatives when available. The error highlights the
 * token's location and suggests possible corrections.
 */
void Parser::emitUnexpectedTokenError(
    const Token &token, const std::vector<std::string> &expectedTokens) {
  auto builder =
      error(std::format("unexpected token `{}`", token.getLexeme()))
          .with_primary_label(spanFromToken(token), "unexpected token");

  if (!expectedTokens.empty()) {
    std::string suggestion = "expected ";
    for (size_t i = 0; i < expectedTokens.size(); ++i) {
      if (i > 0) {
        suggestion += i == expectedTokens.size() - 1 ? " or " : ", ";
      }
      suggestion += "`" + expectedTokens[i] + "`";
    }
    builder.with_help(suggestion);
  }

  builder.emit(*diagnosticManager);
}

/**
 * Emits an "unclosed delimiter" error with contextual guidance.
 *
 * @param opening_token The opening delimiter token (e.g., '{', '(') that wasn't
 * closed
 * @param expected_closing The matching closing delimiter that was expected
 *
 * Creates an error message indicating an unclosed delimiter, highlights the
 * opening delimiter's location, suggests the required closing delimiter, and
 * adds a note about proper delimiter matching.
 */
void Parser::emitUnclosedDelimiterError(const Token &openingToken,
                                        const std::string &expectedClosing) {
  error("unclosed delimiter")
      .with_primary_label(
          spanFromToken(openingToken),
          std::format("unclosed `{}`", openingToken.getLexeme()))
      .with_help(
          std::format("expected `{}` to close this delimiter", expectedClosing))
      .with_note("delimiters must be properly matched")
      .emit(*diagnosticManager);
}

/**
 * Synchronizes the parser to the next statement boundary after an error.
 *
 * @return true if synchronized to a statement boundary, false if reached EOF
 *
 * Recovery strategy:
 * 1. Skips the current erroneous token
 * 2. Advances through tokens until encountering:
 *    - Statement starters (function, class, return, if, while, for)
 *    - Statement terminators (semicolon)
 *    - End of file
 * This minimizes cascading errors by resuming at logical statement boundaries.
 */
bool Parser::SyncToTopLvl() {
  return syncTo({TokenKind::tokFun, TokenKind::TokStruct, TokenKind::TokEnum});
}

/**
 * Synchronizes the parser to the next statement boundary after an error.
 *
 * @return true if synchronized to a statement boundary, false if reached EOF
 *
 * Recovery strategy:
 * 1. Skips the current erroneous token
 * 2. Advances through tokens until encountering:
 *    - Statement starters (function, class, return, if, while, for)
 *    - Statement block terminators (closing brace)
 * This minimizes cascading errors by resuming at logical statement boundaries.
 */
bool Parser::SyncToStmt() {
  return syncTo({TokenKind::tokRightBrace, TokenKind::tokReturn,
                 TokenKind::tokIf, TokenKind::tokWhile, TokenKind::tokFor,
                 TokenKind::tokLet});
}

/**
 * Synchronizes the parser to one of specified token types.
 *
 * @param target_tokens List of token types to synchronize to
 * @return true if any target token was found before EOF, false otherwise
 *
 * Advances through the token stream until encountering one of the specified
 * target tokens. Used for context-specific recovery (e.g., block endings).
 */
bool Parser::syncTo(const std::initializer_list<TokenKind> targetTokens) {
  while (!atEOF()) {
    for (const TokenKind target : targetTokens) {
      if (peekToken().getTy() == target) {
        return true;
      }
    }
    advanceToken();
  }
  return false; // Reached EOF without finding targets
}

/**
 * Synchronizes the parser to a specific token type.
 *
 * @param target_token Token type to synchronize to
 * @return true if the target token was found before EOF, false otherwise
 *
 * Efficiently skips tokens until the exact specified token type is found.
 * Useful for recovering from errors where a specific closing token is expected.
 */
bool Parser::syncTo(const TokenKind targetToken) {
  while (!atEOF() && peekToken().getTy() != targetToken) {
    advanceToken();
  }
  return !atEOF(); // Found target unless EOF reached
}

} // namespace phi
