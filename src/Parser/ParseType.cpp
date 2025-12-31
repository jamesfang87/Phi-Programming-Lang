#include "Parser/Parser.hpp"

#include <cassert>
#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

#include "AST/TypeSystem/Type.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

std::optional<TypeRef> Parser::parseType() {
  enum class Indirection : uint8_t { Ptr, Ref, None };
  // Map of primitive type names to their enum representations
  static const std::unordered_map<std::string_view, BuiltinTy::Kind>
      PrimitiveMap = {{"i8", BuiltinTy::i8},         {"i16", BuiltinTy::i16},
                      {"i32", BuiltinTy::i32},       {"i64", BuiltinTy::i64},
                      {"u8", BuiltinTy::u8},         {"u16", BuiltinTy::u16},
                      {"u32", BuiltinTy::u32},       {"u64", BuiltinTy::u64},
                      {"f32", BuiltinTy::f32},       {"f64", BuiltinTy::f64},
                      {"string", BuiltinTy::String}, {"char", BuiltinTy::Char},
                      {"bool", BuiltinTy::Bool},     {"null", BuiltinTy::Null}};

  // a parenthesis indicates a tuple
  if (peekToken().getKind() == TokenKind::OpenParen) {
    SrcLocation Start = peekToken().getStart();
    std::optional<std::vector<TypeRef>> Temp = parseValueList<TypeRef>(
        TokenKind::OpenParen, TokenKind::CloseParen, &Parser::parseType);

    if (!Temp) {
      return std::nullopt;
    }

    SrcLocation End = peekToken().getEnd();
    return TypeCtx::getTuple(*Temp, SrcSpan(Start, End));
  }

  // we look for the type of indirection
  Indirection Kind = Indirection::None;
  std::optional<SrcSpan> IndirectionSpan;
  switch (peekToken().getKind()) {
  case TokenKind::Amp:
    Kind = Indirection::Ref;
    IndirectionSpan = peekToken().getSpan();
    advanceToken();
    break;
  case TokenKind::Star:
    Kind = Indirection::Ptr;
    IndirectionSpan = peekToken().getSpan();
    advanceToken();
    break;
  case TokenKind::Identifier:
    Kind = Indirection::None;
    break;
  default:
    // Validate token is either primitive type or identifier
    if (!PrimitiveMap.contains(peekToken().getLexeme())) {
      error(std::format("invalid token found: {}", peekToken().getLexeme()))
          .with_primary_label(peekToken().getSpan(),
                              "expected a valid type here")
          .with_help("valid types include: int, float, bool, string, or custom "
                     "type names")
          .with_note(
              "types must be either primitive types or valid identifiers")
          .with_code("E0030")
          .emit(*DiagnosticsMan);
      return std::nullopt;
    }
    Kind = Indirection::None;
  }

  const std::string Id = peekToken().getLexeme();
  const SrcSpan Span = peekToken().getSpan();
  const auto It = PrimitiveMap.find(Id);

  advanceToken();

  TypeRef Base = (It == PrimitiveMap.end())
                     ? TypeCtx::getAdt(Id, nullptr, Span)
                     : TypeCtx::getBuiltin(It->second, Span);
  switch (Kind) {
  case Indirection::Ptr:
    return TypeCtx::getPtr(Base, *IndirectionSpan);
  case Indirection::Ref:
    return TypeCtx::getRef(Base, *IndirectionSpan);
  case Indirection::None:
    return Base;
  }
}

} // namespace phi
