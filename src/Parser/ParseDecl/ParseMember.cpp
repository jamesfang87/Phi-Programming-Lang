#include "Parser/Parser.hpp"

#include <llvm/ADT/ScopeExit.h>

#include <optional>

namespace phi {

std::optional<Visibility> Parser::parseAdtMemberVisibility() {
  switch (peekKind()) {
  case TokenKind::PublicKw:
    advanceToken();
    return Visibility::Public;
  case TokenKind::FunKw:
  case TokenKind::Identifier:
    return Visibility::Private;
  default:
    emitUnexpectedTokenError(peekToken());
    syncTo({TokenKind::PublicKw, TokenKind::FunKw, TokenKind::Identifier,
            TokenKind::CloseBrace});
    return std::nullopt;
  }
}

std::unique_ptr<MethodDecl> Parser::parseMethodDecl(std::string ParentName,
                                                    Visibility Vis) {
  assert(advanceToken().getKind() == TokenKind::FunKw);

  // Validate function name
  if (peekToken().getKind() != TokenKind::Identifier) {
    error("invalid function name")
        .with_primary_label(peekToken().getSpan(),
                            "expected function name here")
        .with_secondary_label(peekToken().getSpan(), "after `fun` keyword")
        .with_help("function names must be valid identifiers")
        .with_note("identifiers must start with a letter or underscore")
        .emit(*Diags);
    return nullptr;
  }
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  std::optional<std::vector<std::unique_ptr<TypeArgDecl>>> TypeArgs;
  if (peekKind() == TokenKind::OpenCaret) {
    TypeArgs = parseTypeArgDecls();
    if (!TypeArgs) {
      return nullptr;
    }

    for (auto &Arg : *TypeArgs)
      ValidGenerics.push_back(Arg.get());
  }

  // Schedule cleanup at the end of this function
  auto Cleanup = llvm::make_scope_exit([&] {
    if (TypeArgs) {
      for (size_t i = 0; i < TypeArgs->size(); ++i)
        ValidGenerics.pop_back();
    }
  });

  // Parse parameter list
  auto Params =
      parseList<ParamDecl>(TokenKind::OpenParen, TokenKind::CloseParen, [&] {
        if (peekToken(1).getKind() != TokenKind::ThisKw) {
          return parseParamDecl();
        }

        auto Constness = parseMutability();
        auto Base = TypeCtx::getAdt(ParentName, nullptr, peekToken().getSpan());
        auto Ty = TypeCtx::getRef(Base, peekToken().getSpan());
        return std::make_unique<ParamDecl>(advanceToken().getSpan(), *Constness,
                                           "this", Ty);
      });
  if (!Params)
    return nullptr;
  if (Params->front()->getId() != "this") {
    error("first parameter of method declaration must be `this`")
        .with_primary_label(Params->front()->getSpan())
        .emit(*Diags);
  }

  // Handle optional return type
  TypeRef ReturnTy = TypeCtx::getBuiltin(BuiltinTy::Null, Span);
  if (matchToken(TokenKind::Arrow)) {
    auto Res = parseType();
    if (!Res)
      return nullptr;
    ReturnTy = *Res;
  }

  // Parse function body
  auto Body = parseBlock();
  if (!Body)
    return nullptr;

  return std::make_unique<MethodDecl>(Span, Vis, Id, std::move(TypeArgs),
                                      std::move(*Params), ReturnTy,
                                      std::move(Body));
}

std::unique_ptr<FieldDecl> Parser::parseFieldDecl(uint32_t FieldIndex,
                                                  Visibility Vis) {
  auto Field = parseBinding({.Type = Policy::Required});
  if (!Field)
    return nullptr;

  auto &[Span, Id, Type, Init] = *Field;

  // Validate semicolon terminator
  if (advanceToken().getKind() != TokenKind::Semicolon) {
    error("missing semicolon after field declaration")
        .with_primary_label(peekToken().getSpan(), "expected `;` here")
        .with_help("field declarations must end with a semicolon")
        .with_suggestion(peekToken().getSpan(), ";", "add semicolon")
        .emit(*Diags);
    return nullptr;
  }

  return std::make_unique<FieldDecl>(Span, FieldIndex, Vis, Id, *Type,
                                     std::move(Init));
}

std::unique_ptr<StructDecl> Parser::parseAnonymousStruct() {
  SrcLocation Start = peekToken().getStart();
  uint32_t FieldIndex = 0;
  auto Fields = parseList<FieldDecl>(
      TokenKind::OpenBrace, TokenKind::CloseBrace,
      [&] -> std::unique_ptr<FieldDecl> {
        auto Field = parseBinding({.Type = Required, .Init = Forbidden});
        if (!Field)
          return nullptr;

        auto &[Span, Name, Type, _] = *Field;
        return std::make_unique<FieldDecl>(
            Span, FieldIndex++, Visibility::Public, Name, *Type, nullptr);
      });
  SrcLocation End = peekToken(-1).getEnd();

  // Generate a unique name for the desugared anonymous struct
  static uint32_t AnonymousStructCounter = 0;
  std::string StructId = std::format("@struct_{}", ++AnonymousStructCounter);
  std::vector<std::unique_ptr<MethodDecl>> Methods;
  return std::make_unique<StructDecl>(SrcSpan(Start, End), Visibility::Public,
                                      StructId, std::nullopt,
                                      std::move(*Fields), std::move(Methods));
}

std::unique_ptr<VariantDecl> Parser::parseVariantDecl() {
  assert(peekToken().getKind() == TokenKind::Identifier);
  SrcSpan Span = peekToken().getSpan();
  std::string Id = advanceToken().getLexeme();

  // Semicolon indicates a variant with no payload;
  // parsing is trivial so we return early
  if (matchToken(TokenKind::Semicolon)) {
    return std::make_unique<VariantDecl>(Span, std::move(Id), std::nullopt);
  }

  // anything else is unexpected: variants must be followed by ':' or ';'
  if (!expectToken(TokenKind::Colon)) {
    return nullptr;
  }

  // Now the current token must be a Colon
  std::optional<TypeRef> PayloadType = std::nullopt;
  if (peekKind() != TokenKind::OpenBrace) {
    PayloadType = parseType();
  } else {
    auto Res = parseAnonymousStruct();
    if (!Res)
      return nullptr;

    PayloadType = TypeCtx::getAdt(Res->getId(), Res.get(), Res->getSpan());
    Ast.push_back(std::move(Res));
  }

  if (matchToken(TokenKind::Semicolon)) {
    return (PayloadType) ? std::make_unique<VariantDecl>(
                               VariantDecl(Span, std::move(Id), PayloadType))
                         : nullptr;
  }

  // missing comma/brace — emit diagnostic and try to recover
  error("missing semicolon after enum variant declaration")
      .with_primary_label(peekToken().getSpan(), "expected `;` here")
      .with_help("enum variant declarations must end with a semicolon")
      .with_suggestion(peekToken().getSpan(), ",", "add semicolon")
      .emit(*Diags);

  // consume the unexpected token to avoid infinite loop and recover
  advanceToken();
  return nullptr;
}

} // namespace phi
