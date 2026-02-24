#include "Parser/Parser.hpp"

#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

Parser::Parser(std::vector<Token> Tokens, DiagnosticManager *DiagnosticMan)
    : Tokens(std::move(Tokens)), Diags(std::move(DiagnosticMan)) {
  TokenIt = this->Tokens.begin();
}

std::optional<Parser::ModulePathInfo> Parser::parseModulePath() {
  SrcLocation PathStart, PathEnd;
  std::vector<std::string> Path = {"$main"};
  std::string PathStr;
  // first ID
  expectToken(TokenKind(TokenKind::Identifier), "Module path", false);
  Path = {peekToken().getLexeme()};
  PathStr += peekToken().getLexeme();
  PathStart = advanceToken().getSpan().Start;

  // other double colon separated IDs
  while (matchToken(TokenKind::DoubleColon)) {
    PathStr += "::";
    if (!expectToken(TokenKind(TokenKind::Identifier), "Module path", false)) {
      return std::nullopt;
    }
    Path.push_back(peekToken().getLexeme());
    PathStr += advanceToken().getLexeme();
  }
  PathEnd = peekToken(-1).getSpan().End;
  return ModulePathInfo{PathStr, Path, SrcSpan(PathStart, PathEnd)};
}

std::unique_ptr<ModuleDecl> Parser::parse() {
  // if the file is empty, return an empty module
  static int64_t AnonymousModCounter = 0;
  std::string PathStr = std::format("@AnonymousModule{}", AnonymousModCounter);
  std::vector<std::string> Path = {PathStr};
  SrcSpan Span = Tokens.front().getSpan();
  if (atEOF()) {
    std::vector<std::unique_ptr<ImportStmt>> NoImports;
    return std::make_unique<ModuleDecl>(Span, Visibility::Public,
                                        std::move(PathStr), std::move(Path),
                                        std::move(Ast), std::move(NoImports));
  }

  // otherwise, we check to see if the first token is a module decl.
  // module decls are optional, so there may not be one
  // if there isn't then, we create a default module and parse as usual.
  // Default module information
  if (matchToken(TokenKind::ModuleKw)) {
    auto Res = parseModulePath();
    if (!Res) {
      syncToTopLvl();
    } else {
      auto &[ParsedPathStr, ParsedPath, ParsedSpan] = *Res;
      Path = std::move(ParsedPath);
      PathStr = std::move(ParsedPathStr);
      Span = ParsedSpan;
    }
    expectToken(TokenKind::Semicolon);
  }

  std::vector<std::unique_ptr<ImportStmt>> Imports;
  while (!atEOF()) {
    auto Visibility = parseItemVisibility();
    if (!Visibility)
      continue;

    std::unique_ptr<ItemDecl> Res = nullptr;
    switch (peekKind()) {
    case TokenKind::FunKw:
      Res = parseFunDecl(*Visibility);
      break;
    case TokenKind::StructKw:
      Res = parseStructDecl(*Visibility);
      break;
    case TokenKind::EnumKw:
      Res = parseEnumDecl(*Visibility);
      break;
    case TokenKind::ImportKw:
      if (auto Import = parseImportStmt())
        Imports.push_back(std::move(Import));
      else
        syncToTopLvl();
      break;
    default:
      emitUnexpectedTokenError(peekToken(),
                               {"fun", "struct", "enum", "import"});
    }

    if (Res)
      Ast.push_back(std::move(Res));
    else
      syncToTopLvl(); // Error recovery
  }

  return std::make_unique<ModuleDecl>(Span, Visibility::Public,
                                      std::move(PathStr), std::move(Path),
                                      std::move(Ast), std::move(Imports));
}

} // namespace phi
