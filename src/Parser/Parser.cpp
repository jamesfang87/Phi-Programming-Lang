#include "Parser/Parser.hpp"

#include <memory>

#include "Lexer/TokenKind.hpp"

namespace phi {

Parser::Parser(const std::string_view Src, const std::string_view Path,
               std::vector<Token> Tokens,
               std::shared_ptr<DiagnosticManager> DiagnosticMan)
    : Path(Path), Tokens(Tokens), TokenIt(Tokens.begin()),
      DiagnosticsMan(std::move(DiagnosticMan)) {

  // Register source file with diagnostic manager
  if (this->DiagnosticsMan->source_manager()) {
    this->DiagnosticsMan->source_manager()->addSrcFile(std::string(Path), Src);
  }
}

std::vector<std::unique_ptr<Decl>> Parser::parse() {
  while (!atEOF()) {
    std::unique_ptr<Decl> Res = nullptr;
    switch (peekKind()) {
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
      syncToTopLvl(); // Error recovery
    }

    if (Res)
      Ast.push_back(std::move(Res));
    else
      syncToTopLvl(); // Error recovery
  }

  llvm::sort(Ast, [](const std::unique_ptr<Decl> &LHS,
                     const std::unique_ptr<Decl> &RHS) {
    if (llvm::isa<StructDecl>(LHS.get()) && !llvm::isa<StructDecl>(RHS.get()))
      return true;

    return false;
  });

  return std::move(Ast);
}

} // namespace phi
