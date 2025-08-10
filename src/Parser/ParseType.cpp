#include "Parser/Parser.hpp"

#include <optional>
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
  // Map of primitive type names to their enum representations
  const std::unordered_map<std::string, Type::PrimitiveKind> PrimitiveMap = {
      {"i8", Type::PrimitiveKind::I8Kind},
      {"i16", Type::PrimitiveKind::I16Kind},
      {"i32", Type::PrimitiveKind::I32Kind},
      {"i64", Type::PrimitiveKind::I64Kind},
      {"u8", Type::PrimitiveKind::U8Kind},
      {"u16", Type::PrimitiveKind::U16Kind},
      {"u32", Type::PrimitiveKind::U32Kind},
      {"u64", Type::PrimitiveKind::U64Kind},
      {"f32", Type::PrimitiveKind::F32Kind},
      {"f64", Type::PrimitiveKind::F64Kind},
      {"string", Type::PrimitiveKind::StringKind},
      {"char", Type::PrimitiveKind::CharKind},
      {"bool", Type::PrimitiveKind::BoolKind},
      {"null", Type::PrimitiveKind::NullKind}};

  const std::string Id = peekToken().getLexeme();
  const auto It = PrimitiveMap.find(Id);

  // Validate token is either primitive type or identifier
  if (It == PrimitiveMap.end() &&
      peekToken().getKind() != TokenKind::IdentifierKind) {
    error(std::format("invalid token found: {}", peekToken().getLexeme()))
        .with_primary_label(spanFromToken(peekToken()),
                            "expected a valid type here")
        .with_help("valid types include: int, float, bool, string, or custom "
                   "type names")
        .with_note("types must be either primitive types or valid identifiers")
        .with_code("E0030")
        .emit(*DiagnosticsMan);
    return std::nullopt;
  }

  advanceToken();
  return ((It == PrimitiveMap.end()) ? Type(Id) : Type(It->second));
}

} // namespace phi
