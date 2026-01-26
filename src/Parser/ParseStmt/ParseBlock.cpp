#include "Parser/Parser.hpp"

#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

/**
 * Parses a block of statements enclosed in braces.
 *
 * @return std::unique_ptr<Block> Block AST or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 *
 * Handles:
 * - { statement1; statement2; ... }
 *
 * Performs extensive error recovery and validation:
 * - Validates opening/closing braces
 * - Recovers from nested declaration errors
 * - Skips invalid tokens with detailed error messages
 * - Continues parsing after recoverable errors
 */
std::unique_ptr<Block> Parser::parseBlock() {
  // Validate opening brace
  expectToken(TokenKind::OpenBrace);

  // Parse statements until closing brace
  std::vector<std::unique_ptr<Stmt>> Stmts;
  while (!matchToken(TokenKind::CloseBrace)) {
    if (peekToken().getKind() == TokenKind::Eof) {
      emitUnclosedDelimiterError(peekToken(), "}");
      return nullptr;
    }

    // Parse valid statements
    if (auto Res = parseStmt()) {
      Stmts.push_back(std::move(Res));
      continue;
    }

    syncToStmt();
  }

  return std::make_unique<Block>(std::move(Stmts));
}

} // namespace phi
