#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <string>

#include "AST/Decl.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {
std::unique_ptr<StructDecl> Parser::parseStructDecl() {
  assert(peekToken().getKind() == TokenKind::StructKw);
  SrcLocation Loc = advanceToken().getStart();

  std::string Id = advanceToken().getLexeme();

  if (peekToken().getKind() != TokenKind::OpenBrace) {
    emitUnexpectedTokenError(peekToken(), {"{"});
  }
  advanceToken();

  std::vector<MethodDecl> Methods;
  std::vector<FieldDecl> Fields;
  while (!atEOF() && peekToken().getKind() != TokenKind::CloseBrace) {
    Token Check = peekToken();
    switch (Check.getKind()) {
    case TokenKind::PublicKw:
      Check = peekToken(1);
      break;
    case TokenKind::FunKw:
    case TokenKind::Identifier:
      break; // we don't need to do anything here
    default:
      emitUnexpectedTokenError(peekToken());
      syncTo({TokenKind::PublicKw, TokenKind::FunKw, TokenKind::Identifier,
              TokenKind::CloseBrace});
      break;
    }

    if (Check.getKind() == TokenKind::FunKw) {
      auto Res = parseStructMethodDecl();
      if (Res) {
        Methods.push_back(std::move(*Res));
      } else {
        syncTo({TokenKind::FunKw, TokenKind::VarKw, TokenKind::ConstKw,
                TokenKind::OpenBrace});
      }
    } else if (Check.getKind() == TokenKind::Identifier) {
      auto Res = parseFieldDecl();
      if (Res) {
        Fields.push_back(std::move(*Res));
      } else {
        syncTo({TokenKind::FunKw, TokenKind::VarKw, TokenKind::ConstKw,
                TokenKind::OpenBrace});
      }
    }
  }

  advanceToken();

  return std::make_unique<StructDecl>(Loc, Id, std::move(Fields),
                                      std::move(Methods));
}

std::optional<FieldDecl> Parser::parseFieldDecl() {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return std::nullopt;
  auto [VarLoc, Id, DeclType] = *Binding;

  // Validate assignment operator
  if (advanceToken().getKind() != TokenKind::Equals) {
    return FieldDecl{VarLoc, Id, DeclType, nullptr, IsPrivate};
  }

  // Parse initializer expression
  auto Init = parseExpr();
  if (!Init)
    return std::nullopt;

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after variable declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("variable declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .with_code("E0025")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }

  return FieldDecl{VarLoc, Id, DeclType, std::move(Init), IsPrivate};
}

std::optional<MethodDecl> Parser::parseStructMethodDecl() {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  assert(peekToken().getKind() == TokenKind::FunKw);
  auto Res = parseFunDecl();
  if (!Res) {
    return std::nullopt;
  }

  MethodDecl Method(std::move(*Res), IsPrivate);

  return std::optional<MethodDecl>{std::move(Method)};
}

} // namespace phi
