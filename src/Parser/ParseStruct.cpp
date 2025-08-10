#include "AST/Decl.hpp"
#include "Lexer/TokenKind.hpp"
#include "Parser/Parser.hpp"
#include "SrcManager/SrcLocation.hpp"
#include <cassert>
#include <memory>
#include <print>
#include <string>

namespace phi {
std::unique_ptr<StructDecl> Parser::parseStructDecl() {
  assert(peekToken().getKind() == TokenKind::StructKwKind);
  SrcLocation Loc = advanceToken().getStart();

  std::string Id = advanceToken().getLexeme();
  std::println("{}", Id);

  if (peekToken().getKind() != TokenKind::OpenBraceKind) {
    error("Expected '{' after struct identifier");
  }
  advanceToken();

  std::vector<FunDecl> Methods;
  std::vector<FieldDecl> Fields;
  while (!atEOF() && peekToken().getKind() != TokenKind::CloseBraceKind) {
    if (peekToken().getKind() == TokenKind::FunKwKind) {
      auto res = parseStructMethodDecl();
      if (res) {
        Methods.push_back(std::move(*res));
      } else {
        syncTo({TokenKind::FunKwKind, TokenKind::VarKwKind,
                TokenKind::ConstKwKind, TokenKind::OpenBraceKind});
      }
    } else if (peekToken().getKind() == TokenKind::VarKwKind ||
               peekToken().getKind() == TokenKind::ConstKwKind) {
      auto res = parseFieldDecl();
      if (res) {
        Fields.push_back(std::move(*res));
      } else {
        syncTo({TokenKind::FunKwKind, TokenKind::VarKwKind,
                TokenKind::ConstKwKind, TokenKind::OpenBraceKind});
      }
    }
  }

  advanceToken();

  return std::make_unique<StructDecl>(Loc, Id, std::move(Fields),
                                      std::move(Methods));
}

std::optional<FieldDecl> Parser::parseFieldDecl() {
  SrcLocation Loc = peekToken().getStart();
  bool IsConst;
  if (peekToken().getKind() == TokenKind::ConstKwKind) {
    IsConst = true;
    advanceToken();
  } else if (peekToken().getKind() == TokenKind::VarKwKind) {
    IsConst = false;
    advanceToken();
  } else {
    emitUnexpectedTokenError(peekToken(), {"var", "const"});
    return std::nullopt;
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return std::nullopt;
  auto [VarLoc, Id, type] = *Binding;

  // Validate assignment operator
  if (advanceToken().getKind() != TokenKind::EqualsKind) {
    return FieldDecl{VarLoc, Id, type, IsConst, nullptr};
  }

  // Parse initializer expression
  auto Init = parseExpr();
  if (!Init)
    return std::nullopt;

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::SemicolonKind) {
    error("missing semicolon after variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("variable declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0025")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }

  return FieldDecl{VarLoc, Id, type, IsConst, std::move(Init)};
}

std::optional<FunDecl> Parser::parseStructMethodDecl() {
  assert(peekToken().getKind() == TokenKind::FunKwKind);
  auto Res = parseFunDecl();
  if (!Res) {
    return std::nullopt;
  }

  return std::optional<FunDecl>{std::move(*Res)};
}

} // namespace phi
