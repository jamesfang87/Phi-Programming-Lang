#include "Parser/Parser.hpp"

#include <cassert>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>

#include "AST/Nodes/Stmt.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
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
      auto Res = parseMethodDecl(Id);
      if (Res) {
        Methods.emplace_back(std::move(*Res));
      } else {
        syncTo({TokenKind::FunKw, TokenKind::VarKw, TokenKind::ConstKw,
                TokenKind::OpenBrace});
      }
    } else if (Check.getKind() == TokenKind::Identifier) {
      auto Res = parseFieldDecl(FieldIndex++);
      if (Res) {
        Fields.emplace_back(std::move(*Res));
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

std::optional<FieldDecl> Parser::parseFieldDecl(uint32_t FieldIndex) {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  auto Binding = parseTypedBinding();
  if (!Binding)
    return std::nullopt;
  auto [VarLoc, Id, DeclType] = *Binding;
  assert(DeclType);

  std::unique_ptr<Expr> Init;

  if (peekToken().getKind() == TokenKind::Equals) {
    advanceToken(); // consume '='
    Init = parseExpr();
    if (!Init)
      return std::nullopt;
  }

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after field declaration")
        .with_primary_label(peekToken().getSpan(), "expected `;` here")
        .with_help("field declarations must end with a semicolon")
        .with_suggestion(peekToken().getSpan(), ";", "add semicolon")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }

  return std::optional<FieldDecl>(std::in_place, VarLoc, Id, *DeclType,
                                  std::move(Init), IsPrivate, FieldIndex);
}

std::optional<MethodDecl> Parser::parseMethodDecl(std::string ParentName) {
  bool IsPrivate = true;
  if (peekToken().getKind() == TokenKind::PublicKw) {
    IsPrivate = false;
    advanceToken();
  }

  Token FunKw = advanceToken(); // Eat 'fun'
  const SrcSpan &FunKwSpan = FunKw.getSpan();

  // Validate function name
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(peekToken().getSpan(),
                            "expected function name here")
        .with_secondary_label(FunKw.getSpan(), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }
  std::string Id = peekToken().getLexeme();
  SrcSpan IdSpan = advanceToken().getSpan();

  // Parse parameter list
  auto Params = parseList<ParamDecl>(
      TokenKind::OpenParen, TokenKind::CloseParen,
      [&](Parser *P) -> std::unique_ptr<ParamDecl> {
        if (P->peekToken(1).getKind() != TokenKind::ThisKw) {
          return P->parseParamDecl();
        }

        // TODO: emit error if not const or var
        bool IsConst = P->peekToken().getKind() == TokenKind::ConstKw;
        P->advanceToken();
        auto Base = TypeCtx::getAdt(ParentName, nullptr, peekToken().getSpan());
        auto T = TypeCtx::getRef(Base, peekToken().getSpan());
        return std::make_unique<ParamDecl>(P->advanceToken().getStart(), "this",
                                           T, IsConst);
      });

  if (!Params)
    return std::nullopt;

  for (auto &Param : *Params) {
    assert(Param->hasType());
  }

  // Handle optional return type
  std::optional<TypeRef> ReturnTy = std::nullopt;
  if (peekToken().getKind() == TokenKind::Arrow) {
    advanceToken(); // eat '->'
    ReturnTy.emplace(std::move(*parseType()));
    if (!ReturnTy)
      return std::nullopt;
  } else {
    ReturnTy.emplace(TypeCtx::getBuiltin(BuiltinTy::Null, IdSpan));
  }

  // Parse function body
  auto Body = parseBlock();
  if (!Body)
    return std::nullopt;

  return MethodDecl(FunKwSpan.Start, std::move(Id), *ReturnTy,
                    std::move(Params.value()), std::move(Body), IsPrivate);
}

} // namespace phi
