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

  std::vector<MethodDecl> Methods;
  std::vector<FieldDecl> Fields;
  while (!atEOF() && peekToken().getKind() != TokenKind::CloseBraceKind) {
    Token Check = peekToken();
    switch (Check.getKind()) {
    case TokenKind::PublicKwKind:
      Check = peekToken(1);
      break;
    case TokenKind::FunKwKind:
    case TokenKind::IdentifierKind:
      break; // we don't need to do anything here
    default:
      error("Unexpected token");
      break;
    }

    if (Check.getKind() == TokenKind::FunKwKind) {
      auto Res = parseStructMethodDecl();
      if (Res) {
        Methods.push_back(std::move(*Res));
      } else {
        syncTo({TokenKind::FunKwKind, TokenKind::VarKwKind,
                TokenKind::ConstKwKind, TokenKind::OpenBraceKind});
      }
    } else if (Check.getKind() == TokenKind::IdentifierKind) {
      auto Res = parseFieldDecl();
      if (Res) {
        std::println("{} {}", Res.value().isPrivate(), Res.value().getId());
        Fields.push_back(std::move(*Res));
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
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKwKind) {
    IsPrivate = false;
    advanceToken();
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return std::nullopt;
  auto [VarLoc, Id, Ty] = *Binding;

  // Validate assignment operator
  if (advanceToken().getKind() != TokenKind::EqualsKind) {
    return FieldDecl{VarLoc, Id, Ty, nullptr, IsPrivate};
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

  return FieldDecl{VarLoc, Id, Ty, std::move(Init), IsPrivate};
}

std::optional<MethodDecl> Parser::parseStructMethodDecl() {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKwKind) {
    IsPrivate = false;
    advanceToken();
  }

  assert(peekToken().getKind() == TokenKind::FunKwKind);
  auto Res = parseFunDecl();
  if (!Res) {
    return std::nullopt;
  }

  MethodDecl Method(std::move(*Res), IsPrivate);

  return std::optional<MethodDecl>{std::move(Method)};
}
} // namespace phi
