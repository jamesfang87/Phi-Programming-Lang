#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <string>

#include "AST/Decl.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

std::unique_ptr<EnumDecl> Parser::parseEnumDecl() {
  assert(peekToken().getKind() == TokenKind::EnumKw);
  SrcLocation Loc = advanceToken().getStart();
  std::string Id = advanceToken().getLexeme();

  if (peekToken().getKind() != TokenKind::OpenBrace) {
    emitUnexpectedTokenError(peekToken(), {"{"});
  }
  advanceToken();

  std::vector<VariantDecl> Variants;
  while (!atEOF() && peekToken().getKind() != TokenKind::CloseBrace) {
    if (peekToken().getKind() != TokenKind::Identifier) {
      emitUnexpectedTokenError(peekToken());
      syncTo({TokenKind::Identifier, TokenKind::CloseBrace});
      continue;
    }

    if (auto Res = parseVariantDecl()) {
      Variants.push_back(*Res);
    } else {
      syncTo({TokenKind::Identifier, TokenKind::CloseBrace});
    }
  }

  advanceToken(); // consume the `}`

  return std::make_unique<EnumDecl>(Loc, Id, std::move(Variants));
}

std::optional<VariantDecl> Parser::parseVariantDecl() {
  assert(peekToken().getKind() == TokenKind::Identifier);
  SrcLocation Loc = peekToken().getStart();
  std::string Id = advanceToken().getLexeme();

  switch (peekToken().getKind()) {
  case TokenKind::Comma:
    // typeless
    advanceToken();
    return VariantDecl(Loc, Id, std::nullopt);
  case TokenKind::Colon: {
    advanceToken();
    auto DeclType = parseType();
    auto Kind = peekToken().getKind();
    if (Kind == TokenKind::Comma || Kind == TokenKind::CloseBrace)
      return (DeclType)
                 ? std::optional<VariantDecl>(VariantDecl(Loc, Id, DeclType))
                 : std::nullopt;

    error("missing comma after enum variant declaration")
        .with_primary_label(spanFromToken(peekToken()), "expected `,` here")
        .with_help("enum vairant declarations must end with a comma")
        .with_suggestion(spanFromToken(peekToken()), ",", "add comma")
        .emit(*DiagnosticsMan);
    advanceToken();
    return std::nullopt;
  }
  default:
    emitUnexpectedTokenError(peekToken(), {",", ":"});
    return std::nullopt;
  }
}

} // namespace phi
