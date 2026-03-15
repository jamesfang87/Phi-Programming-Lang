#include "Parser/Parser.hpp"

#include <optional>
#include <string>
#include <utility>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
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

std::optional<TypeRef>
Parser::parseOptTypeAnnotation(const BindingPolicy &Policy) {
  std::optional<TypeRef> Ret;
  if (matchToken(TokenKind::Colon)) {
    if (Policy.Type == Forbidden) {
      error("unexpected type annotation")
          .with_primary_label(peekToken(-2).getSpan(),
                              "type annotations are not allowed here")
          .emit(*Diags);
      return std::nullopt;
    }

    auto Ty = parseType(Policy.AllowPlaceholderForType);
    if (!Ty)
      return std::nullopt;

    Ret = *Ty;
  } else if (Policy.Type == Required) {
    error("missing type annotation")
        .with_primary_label(peekToken(-2).getSpan(),
                            "type annotation required here")
        .with_help("add `: <type>` after the identifier")
        .emit(*Diags);
    return std::nullopt;
  }

  return Ret;
}

std::optional<Parser::TypedBinding>
Parser::parseBinding(const BindingPolicy &Policy) {
  //===------------------------------------------------------------------===//
  // Parse variable names
  //===------------------------------------------------------------------===//

  std::vector<std::string> Names;
  std::vector<std::optional<TypeRef>> TypeAnnotations;
  SrcSpan Span = peekToken().getSpan();

  if (!Policy.AllowDestructuring && peekKind() == TokenKind::OpenParen) {
    error("destructing not allowed in this context")
        .with_primary_label(peekToken().getSpan(),
                            "due to attempt to destructure here")
        .emit(*Diags);
    return std::nullopt;
  }

  if (peekKind() == TokenKind::Identifier) {
    Names.push_back(advanceToken().getLexeme());
    TypeAnnotations.push_back(parseOptTypeAnnotation(Policy));
  } else if (peekKind() == TokenKind::OpenParen) {
    using Var = std::pair<std::string, std::optional<TypeRef>>;
    auto Res = parseValueList<Var>(
        TokenKind::OpenParen, TokenKind::CloseParen,
        [&]() -> std::optional<Var> {
          auto Tok = expectToken(TokenKind::Identifier);
          if (!Tok) {
            return std::nullopt;
          }

          return std::make_optional(
              std::make_pair(Tok->getLexeme(), parseOptTypeAnnotation(Policy)));
        });

    if (!Res) {
      return std::nullopt;
    }

    for (auto &X : *Res) {
      Names.push_back(X.first);
      TypeAnnotations.push_back(X.second);
    }
  }

  //===------------------------------------------------------------------===//
  // Parse initializer
  //===------------------------------------------------------------------===//

  std::unique_ptr<Expr> Init;
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
        .with_primary_label(Span, "initializer for this binding")
        .with_help("add `= <expr>` to initialize this binding")
        .emit(*Diags);
    return std::nullopt;
  }

  return TypedBinding{
      .Span = Span,
      .Names = std::move(Names),
      .Type = std::move(TypeAnnotations),
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

  auto &[Span, Id, Type, _] = *Param;
  assert(Id.size() > 0 && Type.size() > 0); // make sure that we can index
  return std::make_unique<ParamDecl>(Span, *TheConstness, Id[0], *Type[0]);
}

} // namespace phi
