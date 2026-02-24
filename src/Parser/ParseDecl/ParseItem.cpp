#include "Parser/Parser.hpp"

#include <cassert>
#include <cstddef>
#include <memory>
#include <optional>
#include <utility>

#include <llvm/ADT/ScopeExit.h>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

std::optional<Visibility> Parser::parseItemVisibility() {
  switch (peekKind()) {
  case TokenKind::PublicKw:
    advanceToken();
    return Visibility::Public;
  case TokenKind::FunKw:
  case TokenKind::StructKw:
  case TokenKind::EnumKw:
  case TokenKind::ImportKw:
    return Visibility::Private;
  default:
    emitUnexpectedTokenError(peekToken());
    syncTo({TokenKind::PublicKw, TokenKind::FunKw, TokenKind::StructKw,
            TokenKind::EnumKw});
    return std::nullopt;
  }
}

std::optional<std::vector<std::unique_ptr<TypeArgDecl>>>
Parser::parseTypeArgDecls() {
  if (peekKind() != TokenKind::OpenCaret) {
    return std::vector<std::unique_ptr<TypeArgDecl>>{};
  }

  auto Res = parseList<TypeArgDecl>(
      TokenKind::OpenCaret, TokenKind::CloseCaret,
      [&] -> std::unique_ptr<TypeArgDecl> {
        if (!expectToken(TokenKind::Identifier, "type argument", false)) {
          return nullptr;
        }

        auto Span = peekToken().getSpan();
        auto Id = advanceToken().getLexeme();
        return std::make_unique<TypeArgDecl>(Span, Id);
      });
  if (!Res)
    return std::nullopt;

  for (auto &Arg : *Res)
    ValidGenerics.push_back(Arg.get());
  return Res;
}

std::optional<TypeRef> Parser::parseReturnTy(SrcSpan FunSpan) {
  if (matchToken(TokenKind::Arrow)) {
    return parseType(false);
  }

  return TypeCtx::getBuiltin(BuiltinTy::Null, FunSpan);
}

std::unique_ptr<FunDecl> Parser::parseFunDecl(Visibility Vis) {
  assert(advanceToken().getKind() == TokenKind::FunKw);

  // Validate function name
  if (peekKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(peekToken().getSpan(),
                            "expected function name here")
        .with_secondary_label(peekToken().getSpan(), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .emit(*Diags);
    return nullptr;
  }
  std::string Id = peekToken().getLexeme();
  SrcSpan Span = advanceToken().getSpan();

  auto TypeArgs = parseTypeArgDecls();
  if (!TypeArgs) {
    return nullptr;
  }

  // Schedule cleanup at the end of this function
  size_t NumTypeArgs = TypeArgs->size();
  auto Cleanup = llvm::make_scope_exit([&] {
    for (size_t i = 0; i < NumTypeArgs; ++i)
      ValidGenerics.pop_back();
  });

  auto Params = parseList<ParamDecl>(
      TokenKind::OpenParen, TokenKind::CloseParen, &Parser::parseParamDecl);
  if (!Params)
    return nullptr;

  auto ReturnTy = parseReturnTy(Span);
  if (!ReturnTy)
    return nullptr;

  auto Body = parseBlock();
  if (!Body)
    return nullptr;

  return std::make_unique<FunDecl>(std::move(Span), Vis, std::move(Id),
                                   std::move(*TypeArgs), std::move(*Params),
                                   *std::move(ReturnTy), std::move(Body));
}

std::unique_ptr<EnumDecl> Parser::parseEnumDecl(Visibility Vis) {
  assert(advanceToken().getKind() == TokenKind::EnumKw);
  expectToken(TokenKind::Identifier, "", false);
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  auto TypeArgs = parseTypeArgDecls();
  if (!TypeArgs) {
    return nullptr;
  }

  // Schedule cleanup at the end of this function
  size_t NumTypeArgs = TypeArgs->size();
  auto Cleanup = llvm::make_scope_exit([&] {
    for (size_t i = 0; i < NumTypeArgs; ++i)
      ValidGenerics.pop_back();
  });

  expectToken(TokenKind::OpenBrace);
  std::vector<std::unique_ptr<VariantDecl>> Variants;
  std::vector<std::unique_ptr<MethodDecl>> Methods;
  while (!atEOF() && !matchToken(TokenKind::CloseBrace)) {
    auto Visibility = parseAdtMemberVisibility();
    if (!Visibility)
      continue;

    switch (peekKind()) {
    case TokenKind::FunKw:
      if (auto Res = parseMethodDecl(Id, *Visibility)) {
        if (!Res->getParams().empty() &&
            Res->getParams().front()->getId() == "this") {
          Methods.push_back(std::move(Res));
        } else {
          auto MethodId = Res->getId();
          desugarStaticMethod(
              Id, MethodId, *TypeArgs, std::move(Res->getTypeArgs()),
              std::move(Res->getParams()), Res->getReturnType(),
              std::make_unique<Block>(std::move(Res->getBody())),
              Res->getSpan(), Res->getVisibility());
        }
        break;
      }
      goto recover;
    case TokenKind::Identifier:
      if (*Visibility == Visibility::Public) {
        error("Cannot declare a variant as public; they are public by default")
            .with_primary_label(peekToken(-1).getSpan(), "remove this")
            .emit(*Diags);
        goto recover;
      }

      if (auto Res = parseVariantDecl()) {
        Variants.push_back(std::move(Res));
        break;
      }
      goto recover;
    default:
    recover:
      syncTo({TokenKind::FunKw, TokenKind::Identifier, TokenKind::CloseBrace});
      continue;
    }
  }

  return std::make_unique<EnumDecl>(std::move(Span), Vis, std::move(Id),
                                    std::move(*TypeArgs), std::move(Variants),
                                    std::move(Methods));
}

std::unique_ptr<StructDecl> Parser::parseStructDecl(Visibility Vis) {
  assert(advanceToken().getKind() == TokenKind::StructKw);
  expectToken(TokenKind::Identifier, "struct declaration", false);
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  auto TypeArgs = parseTypeArgDecls();
  if (!TypeArgs) {
    return nullptr;
  }

  // Schedule cleanup at the end of this function
  size_t NumTypeArgs = TypeArgs->size();
  auto Cleanup = llvm::make_scope_exit([&] {
    for (size_t i = 0; i < NumTypeArgs; ++i)
      ValidGenerics.pop_back();
  });

  expectToken(TokenKind::OpenBrace);
  uint32_t FieldIndex = 0;
  std::vector<std::unique_ptr<FieldDecl>> Fields;
  std::vector<std::unique_ptr<MethodDecl>> Methods;
  while (!atEOF() && !matchToken(TokenKind::CloseBrace)) {
    auto Visibility = parseAdtMemberVisibility();
    if (!Visibility)
      continue;

    switch (peekKind()) {
    case TokenKind::FunKw:
      if (auto Res = parseMethodDecl(Id, *Visibility)) {
        if (!Res->getParams().empty() &&
            Res->getParams().front()->getId() == "this") {
          Methods.push_back(std::move(Res));
        } else {
          auto MethodId = Res->getId();
          desugarStaticMethod(
              Id, MethodId, *TypeArgs, std::move(Res->getTypeArgs()),
              std::move(Res->getParams()), Res->getReturnType(),
              std::make_unique<Block>(std::move(Res->getBody())),
              Res->getSpan(), Res->getVisibility());
        }
        break;
      }
      goto recover;
    case TokenKind::Identifier:
      if (auto Res = parseFieldDecl(FieldIndex++, *Visibility)) {
        Fields.push_back(std::move(Res));
        break;
      }
      goto recover;
    default:
    recover:
      syncTo({TokenKind::FunKw, TokenKind::Identifier, TokenKind::CloseBrace});
      continue;
    }
  }

  return std::make_unique<StructDecl>(std::move(Span), Vis, std::move(Id),
                                      std::move(*TypeArgs), std::move(Fields),
                                      std::move(Methods));
}

} // namespace phi
