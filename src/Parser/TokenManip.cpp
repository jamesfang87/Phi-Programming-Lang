#include "Parser/Parser.hpp"

namespace phi {

/**
 * Checks if parser has reached end of token stream.
 *
 * @return true if at EOF, false otherwise.
 *
 * Handles edge cases where token iterator might be out of bounds.
 */
bool Parser::atEOF() const {
  return TokenIt >= Tokens.end() || peekToken().getKind() == TokenKind::Eof;
}

/**
 * Peeks at current token without consuming it.
 *
 * @return Token Current token in stream.
 *
 * Returns synthetic EOF token if at end of stream.
 */
Token Parser::peekToken() const {
  if (TokenIt >= Tokens.end()) {
    const SrcLocation EofLoc{.path = "", .line = -1, .col = -1};
    return Token{EofLoc, EofLoc, TokenKind::Eof, ""};
  }
  return *TokenIt;
}

/**
 * Advances token stream and returns current token.
 *
 * @return Token The consumed token.
 */
Token Parser::advanceToken() {
  Token Ret = peekToken();
  if (TokenIt < Tokens.end()) {
    ++TokenIt;
  }
  return Ret;
}

/**
 * Verifies next token matches expected type.
 *
 * @param expected_type Expected token type
 * @param context Optional context string for error messages
 * @return true if token matches, false otherwise (emits error)
 *
 * Automatically advances on match. Emits detailed error on mismatch.
 */
bool Parser::expectToken(const TokenKind Expected, const std::string &Context) {
  if (peekToken().getKind() == Expected) {
    advanceToken();
    return true;
  }

  const std::string Msg = Context.empty() ? "" : " in " + Context;
  emitExpectedFoundError(tyToStr(Expected) + Msg, peekToken());
  return false;
}

/**
 * Conditionally consumes token if it matches type.
 *
 * @param type Token type to match
 * @return true if token matched and was consumed, false otherwise
 */
bool Parser::matchToken(const TokenKind Expected) {
  if (peekToken().getKind() == Expected) {
    advanceToken();
    return true;
  }
  return false;
}

} // namespace phi
