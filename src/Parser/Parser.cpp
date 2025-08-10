#include "Parser/Parser.hpp"

#include "Lexer/TokenKind.hpp"
#include <print>

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
               std::shared_ptr<DiagnosticManager> DiagnosticMan)
    : Path(path), Tokens(tokens), TokenIt(tokens.begin()),
      DiagnosticsMan(std::move(DiagnosticMan)) {

  // Register source file with diagnostic manager
  if (this->DiagnosticsMan->source_manager()) {
    this->DiagnosticsMan->source_manager()->addSrcFile(std::string(path), src);
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
std::vector<std::unique_ptr<Decl>> Parser::parse() {
  while (!atEOF()) {
    switch (peekToken().getKind()) {
    case TokenKind::FunKwKind: {
      if (auto Res = parseFunDecl()) {
        TopLvlDecls.push_back(std::move(Res));
      } else {
        SyncToTopLvl(); // Error recovery
      }
      break;
    }
    case TokenKind::StructKwKind: {
      if (auto Res = parseStructDecl()) {
        TopLvlDecls.push_back(std::move(Res));
      } else {
        SyncToTopLvl(); // Error recovery
      }
      break;
    }
    default:
      emitUnexpectedTokenError(peekToken(), {"fun", "struct"});
      SyncToTopLvl(); // Error recovery
    }
  }

  return std::move(TopLvlDecls);
}

} // namespace phi
