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
std::unique_ptr<Block> Parser::parse_block() {
  // Validate opening brace
  if (peekToken().getTy() != TokenType::tok_open_brace) {
    emit_expected_found_error("{", peekToken());
  }
  advanceToken();

  // Parse statements until closing brace
  std::vector<std::unique_ptr<Stmt>> stmts;
  while (peekToken().getTy() != TokenType::tok_close_brace) {
    if (peekToken().getTy() == TokenType::tok_eof) {
      emit_unclosed_delimiter_error(peekToken(), "}");
      return nullptr;
    }

    if (peekToken().getTy() == TokenType::tok_class ||
        peekToken().getTy() == TokenType::tok_fun) {
      error("top-level declarations are not allowed inside function bodies")
          .with_primary_label(spanFromToken(peekToken()),
                              "top-level declaration here")
          .with_suggestion(spanFromToken(peekToken()), "",
                           "consider moving this to the top level")
          .with_code("E0003")
          .emit(*diagnosticsManager);
      sync_to_stmt();
      continue;
    }

    // Parse valid statements
    auto res = parse_stmt();
    if (res) {
      stmts.push_back(std::move(res));
      continue;
    }

    sync_to_stmt();
  }

  advanceToken(); // eat `}`

  return std::make_unique<Block>(std::move(stmts));
}

} // namespace phi
