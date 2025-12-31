#include "Parser/Parser.hpp"

#include <cassert>
#include <memory>
#include <optional>
#include <string>

#include "AST/Nodes/Stmt.hpp"
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
  std::vector<MethodDecl> Methods;
  while (!atEOF() && peekToken().getKind() != TokenKind::CloseBrace) {
    Token Check = peekToken();
    switch (Check.getKind()) {
    case TokenKind::PublicKw:
      Check = peekToken(1);
      break;
    case TokenKind::FunKw:
    case TokenKind::Identifier:
      break; // valid starts: method or variant
    default:
      emitUnexpectedTokenError(peekToken());
      // recover to next plausible member (method or variant) or end of enum
      syncTo({TokenKind::PublicKw, TokenKind::FunKw, TokenKind::Identifier,
              TokenKind::CloseBrace});
      continue;
    }

    if (Check.getKind() == TokenKind::FunKw) {
      auto Res = parseMethodDecl(Id);
      if (Res) {
        Methods.push_back(std::move(*Res));
      } else {
        // if method parsing failed, resync to a plausible following token
        syncTo(
            {TokenKind::FunKw, TokenKind::Identifier, TokenKind::CloseBrace});
      }
    } else if (Check.getKind() == TokenKind::Identifier) {
      // parse a variant
      if (peekToken().getKind() != TokenKind::Identifier) {
        emitUnexpectedTokenError(peekToken());
        syncTo(
            {TokenKind::Identifier, TokenKind::CloseBrace, TokenKind::FunKw});
        continue;
      }

      if (auto Res = parseVariantDecl()) {
        Variants.push_back(*Res);
      } else {
        // recover to next member or end of enum
        syncTo(
            {TokenKind::Identifier, TokenKind::FunKw, TokenKind::CloseBrace});
      }
    }
  }

  // consume the `}`
  advanceToken();

  return std::make_unique<EnumDecl>(Loc, Id, std::move(Variants),
                                    std::move(Methods));
}

std::optional<VariantDecl> Parser::parseVariantDecl() {
  assert(peekToken().getKind() == TokenKind::Identifier);
  SrcLocation Loc = peekToken().getStart();
  std::string Id = advanceToken().getLexeme();

  switch (peekKind()) {
  case TokenKind::Semicolon:
    advanceToken(); // eat ';'
    return VariantDecl(Loc, std::move(Id), std::nullopt);
  case TokenKind::Colon: {
    advanceToken(); // eat ':'

    std::optional<TypeRef> DeclType;
    if (peekKind() == TokenKind::OpenBrace) {
      // Remember start location of the anonymous struct (the '{' token)
      Token Opening = peekToken();
      SrcLocation Start = Opening.getStart();

      // Parse the list of fields inside { ... }.
      // parseValueList will consume the opening and closing braces.
      auto FieldsRes = parseValueList<TypedBinding>(
          TokenKind::OpenBrace, TokenKind::CloseBrace,
          &Parser::parseTypedBinding, "anonymous struct fields");
      SrcLocation End = peekToken().getEnd();

      if (!FieldsRes) {
        // failed to parse anonymous struct fields; leave DeclType as nullopt
        DeclType = std::nullopt;
      } else {
        // Convert parsed tuples into FieldDecls
        std::vector<FieldDecl> Fields;
        uint32_t FieldIndex = 0;
        for (auto &Field : *FieldsRes) {
          auto [Location, Name, Type] = Field;

          // No initializer for anonymous struct fields; everything is public
          std::unique_ptr<Expr> Init = nullptr;
          bool IsPrivate = false;
          Fields.emplace_back(Location, std::move(Name), *Type, std::move(Init),
                              IsPrivate, FieldIndex++);
        }

        // Generate a unique name for the desugared anonymous struct
        static uint32_t AnonStructCounter = 0;
        std::string AnonName =
            "@anon_struct_" + std::to_string(++AnonStructCounter);
        std::vector<MethodDecl> Methods;
        auto UniPtr = std::make_unique<StructDecl>(
            Start, AnonName, std::move(Fields), std::move(Methods));
        auto *RawPtr = UniPtr.get();
        Ast.push_back(std::move(UniPtr));
        DeclType.emplace(
            TypeCtx::getAdt(AnonName, RawPtr, SrcSpan(Start, End)));
      }
    } else {
      DeclType.emplace(std::move(*parseType()));
    }

    if (matchToken(TokenKind::Semicolon)) {
      return (DeclType) ? std::optional<VariantDecl>(
                              VariantDecl(Loc, std::move(Id), DeclType))
                        : std::nullopt;
    }

    // missing comma/brace — emit diagnostic and try to recover
    error("missing semicolon after enum variant declaration")
        .with_primary_label(peekToken().getSpan(), "expected `;` here")
        .with_help("enum variant declarations must end with a semicolon")
        .with_suggestion(peekToken().getSpan(), ",", "add semicolon")
        .emit(*DiagnosticsMan);

    // consume the unexpected token to avoid infinite loop and recover
    advanceToken();
    return std::nullopt;
  }
  default:
    // anything else is unexpected: variants must be followed by ':' or ';' or
    // be the end
    emitUnexpectedTokenError(peekToken(), {";", ":"});
    return std::nullopt;
  }
}

} // namespace phi
