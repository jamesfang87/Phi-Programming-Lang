#include "Parser/Parser.hpp"

#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Constructs a parser instance.
 *
 * @param src Source code string
 * @param path Source file path
 * @param tokens Token stream from lexer
 * @param diagnostic_manager Shared diagnostic manager
 */
Parser::Parser(const std::string_view src, const std::string_view path,
               std::vector<Token> &tokens,
               std::shared_ptr<DiagnosticManager> diagnosticManager)
    : path(path), tokens(tokens), tokenIt(tokens.begin()),
      diagnosticManager(std::move(diagnosticManager)) {

  // Register source file with diagnostic manager
  if (this->diagnosticManager->source_manager()) {
    this->diagnosticManager->source_manager()->addSrcFile(std::string(path),
                                                          src);
  }
}

/**
 * Main parsing entry point.
 *
 * @return std::pair<std::vector<std::unique_ptr<FunDecl>>, bool>
 *         Pair of function declarations and success status
 *
 * Parses entire token stream, collecting function declarations.
 * Uses error recovery via sync_to() after errors.
 */
std::vector<std::unique_ptr<FunDecl>> Parser::parse() {
  while (!atEOF()) {
    switch (peekToken().getType()) {
    case TokenKind::tokFun: {
      if (auto res = parseFunDecl()) {
        functions.push_back(std::move(res));
      } else {
        SyncToTopLvl(); // Error recovery
      }
      break;
    }
    default:
      emitUnexpectedTokenError(peekToken(), {"fun"});
      SyncToTopLvl(); // Error recovery
    }
  }

  return std::move(functions);
}

} // namespace phi
