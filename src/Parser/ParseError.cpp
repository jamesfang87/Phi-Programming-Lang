#include "Lexer/TokenKind.hpp"
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
void Parser::emitExpectedFoundError(const std::string &Expected,
                                    const Token &FoundToken) {
  error(
      std::format("expected {}, found `{}`", Expected, FoundToken.getLexeme()))
      .with_primary_label(FoundToken.getSpan(),
                          std::format("expected {} here", Expected))
      .emit(*DiagnosticsMan);
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
    const Token &Token, const std::vector<std::string> &ExpectedTokens) {
  auto Builder = error(std::format("unexpected token `{}`", Token.getLexeme()))
                     .with_primary_label(Token.getSpan(), "unexpected token");

  if (!ExpectedTokens.empty()) {
    std::string Suggestion = "expected ";
    for (size_t i = 0; i < ExpectedTokens.size(); ++i) {
      if (i > 0) {
        Suggestion += i == ExpectedTokens.size() - 1 ? " or " : ", ";
      }
      Suggestion += "`" + ExpectedTokens[i] + "`";
    }
    Builder.with_help(Suggestion);
  }

  Builder.emit(*DiagnosticsMan);
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
void Parser::emitUnclosedDelimiterError(const Token &OpeningToken,
                                        const std::string &ExpectedClosing) {
  error("unclosed delimiter")
      .with_primary_label(
          OpeningToken.getSpan(),
          std::format("unclosed `{}`", OpeningToken.getLexeme()))
      .with_help(
          std::format("expected `{}` to close this delimiter", ExpectedClosing))
      .with_note("delimiters must be properly matched")
      .emit(*DiagnosticsMan);
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
  return syncTo({TokenKind::FunKw, TokenKind::StructKw, TokenKind::EnumKw});
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
  return syncTo({TokenKind::CloseBrace, TokenKind::ReturnKw, TokenKind::IfKw,
                 TokenKind::WhileKw, TokenKind::ForKw, TokenKind::VarKw});
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
bool Parser::syncTo(const std::initializer_list<TokenKind> Targets) {
  advanceToken();
  while (!atEOF()) {
    for (const TokenKind target : Targets) {
      if (peekToken().getKind() == target) {
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
bool Parser::syncTo(const TokenKind Target) {
  while (!atEOF() && peekToken().getKind() != Target) {
    advanceToken();
  }
  return !atEOF(); // Found target unless EOF reached
}

} // namespace phi
