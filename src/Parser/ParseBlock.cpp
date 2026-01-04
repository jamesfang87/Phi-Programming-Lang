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
  if (peekToken().getKind() != TokenKind::OpenBrace) {
    emitExpectedFoundError("{", peekToken());
  }
  advanceToken();

  // Parse statements until closing brace
  std::vector<std::unique_ptr<Stmt>> Stmts;
  while (peekToken().getKind() != TokenKind::CloseBrace) {
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

  advanceToken(); // eat `}`

  return std::make_unique<Block>(std::move(Stmts));
}

} // namespace phi
