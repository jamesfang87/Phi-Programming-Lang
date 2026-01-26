#include "Parser/Parser.hpp"

#include <optional>

#include "AST/Nodes/Decl.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

std::optional<Mutability> Parser::parseMutability() {
  if (peekKind() != TokenKind::ConstKw && peekKind() != TokenKind::VarKw) {
    emitUnexpectedTokenError(peekToken(), {"const", "var"});
    return std::nullopt;
  }

  return advanceToken().getKind() == TokenKind::ConstKw ? Mutability::Const
                                                        : Mutability::Var;
}

std::optional<Parser::TypedBinding>
Parser::parseBinding(const BindingPolicy &Policy) {
  // Parse identifier
  expectToken(TokenKind(TokenKind::Identifier), "", false);
  SrcSpan Span = peekToken().getSpan();
  std::string Name = advanceToken().getLexeme();

  std::optional<TypeRef> DeclType;
  std::unique_ptr<Expr> Init;

  //===------------------------------------------------------------------===//
  // Parse type annotation
  //===------------------------------------------------------------------===//

  if (matchToken(TokenKind::Colon)) {
    if (Policy.Type == Forbidden) {
      error("unexpected type annotation")
          .with_primary_label(peekToken(-1).getSpan(),
                              "type annotations are not allowed here")
          .emit(*Diags);
      return std::nullopt;
    }

    auto Ty = parseType();
    if (!Ty)
      return std::nullopt;

    DeclType = *Ty;
  } else if (Policy.Type == Required) {
    error("missing type annotation")
        .with_primary_label(Span, "type annotation required here")
        .with_help("add `: <type>` after the identifier")
        .emit(*Diags);
    return std::nullopt;
  }

  //===------------------------------------------------------------------===//
  // Parse initializer
  //===------------------------------------------------------------------===//

  if (matchToken(TokenKind::Equals)) {
    if (Policy.Init == Forbidden) {
      error("unexpected initializer")
          .with_primary_label(peekToken(-1).getSpan(),
                              "initializers are not allowed here")
          .emit(*Diags);
      return std::nullopt;
    }

    auto Expr = parseExpr();
    if (!Expr)
      return std::nullopt;

    Init = std::move(Expr);
  } else if (Policy.Init == Required) {
    error("missing initializer")
        .with_primary_label(Span, "initializer required here")
        .with_help("add `= <expr>` to initialize this binding")
        .emit(*Diags);
    return std::nullopt;
  }

  return TypedBinding{
      .Span = Span,
      .Name = std::move(Name),
      .Type = std::move(DeclType),
      .Init = std::move(Init),
  };
}

std::unique_ptr<ParamDecl> Parser::parseParamDecl() {
  auto TheConstness = parseMutability();
  if (!TheConstness)
    return nullptr;

  auto Param = parseBinding({.Type = Required, .Init = Forbidden});
  if (!Param)
    return nullptr;

  auto &[Span, Id, DeclType, _] = *Param;
  return std::make_unique<ParamDecl>(Span, *TheConstness, Id, *DeclType);
}

} // namespace phi
