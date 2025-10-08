#include "Parser/Parser.hpp"

#include <cassert>
#include <cstdint>
#include <memory>
#include <optional>
#include <print>
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

  uint32_t FieldIndex = 0;
  std::vector<MethodDecl> Methods;
  std::vector<std::unique_ptr<FieldDecl>> Fields;
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
      auto Res = parseMethodDecl(Id, Loc);
      if (Res) {
        Methods.push_back(std::move(*Res));
      } else {
        syncTo({TokenKind::FunKw, TokenKind::VarKw, TokenKind::ConstKw,
                TokenKind::OpenBrace});
      }
    } else if (Check.getKind() == TokenKind::Identifier) {
      auto Res = parseFieldDecl(FieldIndex++);
      if (Res) {
        Fields.push_back(std::move(Res));
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

std::unique_ptr<FieldDecl> Parser::parseFieldDecl(uint32_t FieldIndex) {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return nullptr;

  auto [VarLoc, Id, DeclType] = *Binding;

  std::unique_ptr<Expr> Init;

  if (peekToken().getKind() == TokenKind::Equals) {
    advanceToken(); // consume '='
    Init = parseExpr();
    if (!Init)
      return nullptr;
  }

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after field declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `;` here")
        .with_help("field declarations must end with a semicolon")
        .with_suggestion(spanFromToken(peekToken()), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return nullptr;
  }

  return std::make_unique<FieldDecl>(VarLoc, Id, DeclType, std::move(Init),
                                     IsPrivate, FieldIndex);
}

std::optional<MethodDecl> Parser::parseMethodDecl(std::string ParentStruct,
                                                  SrcLocation ParentLoc) {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  Token Tok = advanceToken(); // Eat 'fun'
  const SrcLocation &Loc = Tok.getStart();

  // Validate function name
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(spanFromToken(peekToken()),
                            "expected function name here")
        .with_secondary_label(spanFromToken(Tok), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  std::string Id = advanceToken().getLexeme();

  // Parse parameter list
  auto Params = parseList<ParamDecl>(
      TokenKind::OpenParen, TokenKind::CloseParen,
      [&](Parser *P) -> std::unique_ptr<ParamDecl> {
        if (P->peekToken(1).getKind() == TokenKind::ThisKw) {
          // emit error if not const or var
          bool IsConst = P->peekToken().getKind() == TokenKind::ConstKw;
          P->advanceToken();
          Type T = Type::makeReference(
              Type::makeCustom(ParentStruct, ParentLoc), ParentLoc);
          return std::make_unique<ParamDecl>(P->advanceToken().getStart(),
                                             "this", T, IsConst);
        } else {

          return P->parseParamDecl();
        }
      }

  );
  if (!Params)
    return std::nullopt;

  for (auto &Param : *Params) {
    assert(Param->hasType());
  }

  // Handle optional return type
  auto ReturnType = Type::makePrimitive(PrimitiveKind::Null, SrcLocation{});
  if (peekToken().getKind() == TokenKind::Arrow) {
    advanceToken(); // eat '->'
    auto Res = parseType();
    if (!Res.has_value())
      return std::nullopt;
    ReturnType = Res.value();
  }

  // Parse function body
  auto Body = parseBlock();
  if (!Body)
    return std::nullopt;

  return MethodDecl(Loc, std::move(Id), ReturnType, std::move(Params.value()),
                    std::move(Body), IsPrivate);
}

} // namespace phi
