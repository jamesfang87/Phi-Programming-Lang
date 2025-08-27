#include "Lexer/Token.hpp"
#include "Parser/Parser.hpp"
#include "SrcManager/SrcLocation.hpp"

#include <optional>
#include <string>
#include <string_view>
#include <unordered_map>

namespace phi {

/**
 * Parses a type specification from the token stream.
 *
 * @return std::expected<Type, Diagnostic> The parsed type on success, or a
 * diagnostic error on failure.
 *
 * Handles both primitive types (i8, i16, etc.) and custom type identifiers.
 * Emits detailed errors for invalid type tokens, including:
 * - Unexpected tokens in type position
 * - Suggestions for valid primitive types
 * - Notes about type syntax rules
 */
std::optional<Type> Parser::parseType() {
  enum class Indirection : uint8_t { Ptr, Ref, None };
  // Map of primitive type names to their enum representations
  static const std::unordered_map<std::string_view, PrimitiveKind>
      PrimitiveMap = {
          {"i8", PrimitiveKind::I8},         {"i16", PrimitiveKind::I16},
          {"i32", PrimitiveKind::I32},       {"i64", PrimitiveKind::I64},
          {"u8", PrimitiveKind::U8},         {"u16", PrimitiveKind::U16},
          {"u32", PrimitiveKind::U32},       {"u64", PrimitiveKind::U64},
          {"f32", PrimitiveKind::F32},       {"f64", PrimitiveKind::F64},
          {"string", PrimitiveKind::String}, {"char", PrimitiveKind::Char},
          {"bool", PrimitiveKind::Bool},     {"null", PrimitiveKind::Null}};

  Indirection Kind;
  SrcLocation IndrectionLocation;
  switch (peekToken().getKind()) {
  case TokenKind::AmpKind:
    Kind = Indirection::Ref;
    IndrectionLocation = peekToken().getStart();
    advanceToken();
    break;
  case TokenKind::StarKind:
    Kind = Indirection::Ptr;
    IndrectionLocation = peekToken().getStart();
    advanceToken();
    break;
  case TokenKind::IdentifierKind:
    Kind = Indirection::None;
    break;
  default:
    // Validate token is either primitive type or identifier
    if (!PrimitiveMap.contains(peekToken().getLexeme())) {
      error(std::format("invalid token found: {}", peekToken().getLexeme()))
          .with_primary_label(spanFromToken(peekToken()),
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
  const SrcLocation Loc = peekToken().getStart();
  const auto It = PrimitiveMap.find(Id);

  advanceToken();
  Type Base = (It == PrimitiveMap.end()) ? Type::makeCustom(Id, Loc)
                                         : Type::makePrimitive(It->second, Loc);
  switch (Kind) {
  case Indirection::Ptr:
    return Type::makePointer(Base, IndrectionLocation);
  case Indirection::Ref:
    return Type::makeReference(Base, IndrectionLocation);
  case Indirection::None:
    return Base;
  }
}

} // namespace phi
