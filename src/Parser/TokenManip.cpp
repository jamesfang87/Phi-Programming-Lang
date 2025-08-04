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
  return tokenIt >= tokens.end() || peekToken().getTy() == TokenType::tokEOF;
}

/**
 * Peeks at current token without consuming it.
 *
 * @return Token Current token in stream.
 *
 * Returns synthetic EOF token if at end of stream.
 */
Token Parser::peekToken() const {
  if (tokenIt >= tokens.end()) {
    const SrcLocation eofLoc{.path = "", .line = -1, .col = -1};
    return Token{eofLoc, eofLoc, TokenType::tokEOF, ""};
  }
  return *tokenIt;
}

/**
 * Advances token stream and returns current token.
 *
 * @return Token The consumed token.
 */
Token Parser::advanceToken() {
  Token ret = peekToken();
  if (tokenIt < tokens.end()) {
    ++tokenIt;
  }
  return ret;
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
bool Parser::expectToken(const TokenType expectedTy,
                         const std::string &context) {
  if (peekToken().getTy() == expectedTy) {
    advanceToken();
    return true;
  }

  const std::string contextMsg = context.empty() ? "" : " in " + context;
  emitExpectedFoundError(tyToStr(expectedTy) + contextMsg, peekToken());
  return false;
}

/**
 * Conditionally consumes token if it matches type.
 *
 * @param type Token type to match
 * @return true if token matched and was consumed, false otherwise
 */
bool Parser::matchToken(const TokenType type) {
  if (peekToken().getTy() == type) {
    advanceToken();
    return true;
  }
  return false;
}

} // namespace phi
