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
  if (peekToken().getTy() != TokenType::tokLeftBrace) {
    emitExpectedFoundError("{", peekToken());
  }
  advanceToken();

  // Parse statements until closing brace
  std::vector<std::unique_ptr<Stmt>> stmts;
  while (peekToken().getTy() != TokenType::tokRightBrace) {
    if (peekToken().getTy() == TokenType::tokEOF) {
      emitUnclosedDelimiterError(peekToken(), "}");
      return nullptr;
    }

    if (peekToken().getTy() == TokenType::tokClass ||
        peekToken().getTy() == TokenType::tokFun) {
      error("top-level declarations are not allowed inside function bodies")
          .with_primary_label(spanFromToken(peekToken()),
                              "top-level declaration here")
          .with_suggestion(spanFromToken(peekToken()), "",
                           "consider moving this to the top level")
          .with_code("E0003")
          .emit(*diagnosticsManager);
      SyncToStmt();
      continue;
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
