#include "Parser/Parser.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>
#include <string_view>
#include <unordered_map>
#include <utility>
#include <vector>

#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

std::optional<TypeRef> Parser::parseType(bool AllowPlaceholder) {
  // Error type must exist by self
  if (peekKind() == TokenKind::Bang) {
    return TypeCtx::getErr(advanceToken().getSpan());
  }

  // Placeholder type
  if (peekKind() == TokenKind::Wildcard && AllowPlaceholder) {
    return TypeCtx::getVar(VarTy::Any, advanceToken().getSpan());
  }

  if (peekKind() == TokenKind::OpenBracket) {
    auto Open = advanceToken().getSpan().Start;
    auto Contained = parseType(AllowPlaceholder);
    if (!Contained)
      return std::nullopt;

    if (peekKind() == TokenKind::CloseBracket) {
      return TypeCtx::getArray(*Contained,
                               SrcSpan(Open, advanceToken().getSpan().End));
    }
    emitUnexpectedTokenError(peekToken());
    return std::nullopt;
  }

  auto [Kind, IndirectionSpan] = parseIndirection();

  auto Base = parseTypeBase(AllowPlaceholder);
  if (!Base)
    return std::nullopt;

  std::optional<std::vector<TypeRef>> TypeArgs;
  if (peekKind() == TokenKind::OpenCaret) {
    TypeArgs = parseTypeArgList(AllowPlaceholder);
    if (!TypeArgs)
      return std::nullopt;
  }

  // Apply type arguments if present
  if (TypeArgs) {
    Base = TypeCtx::getApplied(*Base, std::move(*TypeArgs), Base->getSpan());
  }

  // Apply Indirection
  switch (Kind) {
  case Indirection::Ptr:
    return TypeCtx::getPtr(*Base, *IndirectionSpan);
  case Indirection::Ref:
    return TypeCtx::getRef(*Base, *IndirectionSpan);
  case Indirection::None:
    return Base;
  }
}

std::pair<Parser::Indirection, std::optional<SrcSpan>>
Parser::parseIndirection() {
  Indirection Kind;
  std::optional<SrcSpan> IndirectionSpan;
  switch (peekKind()) {
  case TokenKind::Amp:
    Kind = Indirection::Ref;
    IndirectionSpan = advanceToken().getSpan();
    break;
  case TokenKind::Star:
    Kind = Indirection::Ptr;
    IndirectionSpan = advanceToken().getSpan();
    break;
  default:
    Kind = Indirection::None;
  }
  return std::make_pair(Kind, IndirectionSpan);
}

std::optional<TypeRef> Parser::parseTypeBase(bool AllowPlaceholder) {
  if (peekKind() == TokenKind::OpenParen) {
    SrcLocation Start = peekToken().getStart();
    std::optional<std::vector<TypeRef>> Temp =
        parseValueList<TypeRef>(TokenKind::OpenParen, TokenKind::CloseParen,
                                [&] { return parseType(AllowPlaceholder); });

    if (!Temp) {
      return std::nullopt;
    }

    SrcLocation End = peekToken().getEnd();
    return TypeCtx::getTuple(*Temp, SrcSpan(Start, End));
  }

  // Map of primitive type names to their enum representations
  static const std::unordered_map<std::string_view, BuiltinTy::Kind>
      PrimitiveMap = {{"i8", BuiltinTy::i8},         {"i16", BuiltinTy::i16},
                      {"i32", BuiltinTy::i32},       {"i64", BuiltinTy::i64},
                      {"u8", BuiltinTy::u8},         {"u16", BuiltinTy::u16},
                      {"u32", BuiltinTy::u32},       {"u64", BuiltinTy::u64},
                      {"f32", BuiltinTy::f32},       {"f64", BuiltinTy::f64},
                      {"string", BuiltinTy::String}, {"char", BuiltinTy::Char},
                      {"bool", BuiltinTy::Bool},     {"null", BuiltinTy::Null}};

  const auto Id = peekToken().getLexeme();
  const auto Span = peekToken().getSpan();
  const auto It = PrimitiveMap.find(Id);

  // Error handling
  if (peekKind() != TokenKind::Identifier && It == PrimitiveMap.end()) {
    error(std::format("invalid token found: {}", peekToken().getLexeme()))
        .with_primary_label(peekToken().getSpan(), "expected a valid type here")
        .with_help("valid types include: i32, f64, bool, string, or custom "
                   "type names")
        .with_note("types must be either primitive types or valid identifiers")
        .emit(*Diags);
    return std::nullopt;
  }

  if (advanceToken().getKind() == TokenKind::Identifier) {
    for (const auto &Gen : ValidGenerics) {
      if (Gen->getId() == Id) {
        return TypeCtx::getGeneric(Gen->getId(), Gen, Span);
      }
    }

    if (BuiltinTyAliases.contains(Id)) {
      return TypeRef(BuiltinTyAliases[Id], Span);
    }
  }

  return (It == PrimitiveMap.end()) ? TypeCtx::getAdt(Id, nullptr, Span)
                                    : TypeCtx::getBuiltin(It->second, Span);
}

std::optional<std::vector<TypeRef>>
Parser::parseTypeArgList(bool AllowPlaceholder) {
  return parseValueList<TypeRef>(TokenKind::OpenCaret, TokenKind::CloseCaret,
                                 [&] { return parseType(AllowPlaceholder); });
}

} // namespace phi
