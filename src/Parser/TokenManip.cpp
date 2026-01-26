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
    const SrcLocation EofLoc{.Path = "", .Line = -1, .Col = -1};
    return Token{EofLoc, EofLoc, TokenKind::Eof, ""};
  }
  return *TokenIt;
}

Token Parser::peekToken(int Offset) const {
  const SrcLocation EofLoc{.Path = "", .Line = -1, .Col = -1};
  auto It = TokenIt + Offset;
  if (It >= Tokens.end())
    return Token{EofLoc, EofLoc, TokenKind::Eof, ""};
  return *It;
}

TokenKind Parser::peekKind() const { return peekToken().getKind(); }

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
 * Only advances on match if Advance = true (by default). Emits detailed error
 * on mismatch.
 */
bool Parser::expectToken(TokenKind::Kind Expected, const std::string &Context,
                         bool Advance) {
  auto Str = TokenKind(Expected).toString();
  if (peekToken().getKind() != Expected) {
    const std::string Msg = Context.empty() ? "" : " in " + Context;
    emitExpectedFoundError(Str + Msg, peekToken());
    return false;
  }

  if (Advance) {
    advanceToken();
  }
  return true;
}

/**
 * Conditionally consumes token if it matches type.
 *
 * @param type Token type to match
 * @return true if token matched and was consumed, false otherwise
 */
bool Parser::matchToken(const TokenKind::Kind Expected) {
  if (peekToken().getKind() == Expected) {
    advanceToken();
    return true;
  }
  return false;
}

} // namespace phi
