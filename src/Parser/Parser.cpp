#include "Parser/Parser.hpp"

#include "Lexer/TokenKind.hpp"
#include <memory>

namespace phi {

/**
 * Constructs a parser instance.
 *
 * @param src Source code string
 * @param path Source file path
 * @param tokens Token stream from lexer
 * @param diagnostic_manager Shared diagnostic manager
 */
Parser::Parser(const std::string_view Src, const std::string_view Path,
               std::vector<Token> &Tokens,
               std::shared_ptr<DiagnosticManager> DiagnosticMan)
    : Path(Path), Tokens(Tokens), TokenIt(Tokens.begin()),
      DiagnosticsMan(std::move(DiagnosticMan)) {

  // Register source file with diagnostic manager
  if (this->DiagnosticsMan->source_manager()) {
    this->DiagnosticsMan->source_manager()->addSrcFile(std::string(Path), Src);
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
    std::unique_ptr<Decl> Res = nullptr;
    switch (peekToken().getKind()) {
    case TokenKind::FunKw:
      Res = parseFunDecl();
      break;
    case TokenKind::StructKw:
      Res = parseStructDecl();
      break;
    case TokenKind::EnumKw:
      Res = parseEnumDecl();
      break;
    default:
      emitUnexpectedTokenError(peekToken(), {"fun", "struct", "enum"});
      SyncToTopLvl(); // Error recovery
    }

    if (Res)
      Ast.push_back(std::move(Res));
    else
      SyncToTopLvl(); // Error recovery
  }

  llvm::sort(Ast, [](const std::unique_ptr<Decl> &LHS,
                     const std::unique_ptr<Decl> &RHS) {
    if (llvm::isa<StructDecl>(LHS.get()) && !llvm::isa<StructDecl>(RHS.get()))
      return true;

    if (llvm::isa<StructDecl>(RHS.get()) && !llvm::isa<StructDecl>(LHS.get()))
      return false;

    return false;
  });

  return std::move(Ast);
}

} // namespace phi
