#include "Lexer/Token.hpp"
#include "Parser/Parser.hpp"

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
  if (peekToken().getType() != TokenKind::tokLeftBrace) {
    emitExpectedFoundError("{", peekToken());
  }
  advanceToken();

  // Parse statements until closing brace
  std::vector<std::unique_ptr<Stmt>> stmts;
  while (peekToken().getType() != TokenKind::tokRightBrace) {
    if (peekToken().getType() == TokenKind::tokEOF) {
      emitUnclosedDelimiterError(peekToken(), "}");
      return nullptr;
    }

    // Parse valid statements
    auto res = parseStmt();
    if (res) {
      stmts.push_back(std::move(res));
      continue;
    }

    SyncToStmt();
  }

  advanceToken(); // eat `}`

  return std::make_unique<Block>(std::move(stmts));
}

} // namespace phi
