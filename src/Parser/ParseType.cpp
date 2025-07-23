#include "Parser/Parser.hpp"

#include <unordered_map>

namespace phi {

std::expected<Type, Diagnostic> Parser::parse_type() {
    const std::unordered_map<std::string, Type::Primitive> primitive_map = {
        {"i8", Type::Primitive::i8},
        {"i16", Type::Primitive::i16},
        {"i32", Type::Primitive::i32},
        {"i64", Type::Primitive::i64},
        {"u8", Type::Primitive::u8},
        {"u16", Type::Primitive::u16},
        {"u32", Type::Primitive::u32},
        {"u64", Type::Primitive::u64},
        {"f32", Type::Primitive::f32},
        {"f64", Type::Primitive::f64},
        {"str", Type::Primitive::str},
        {"char", Type::Primitive::character},
        {"bool", Type::Primitive::boolean},
        {"null", Type::Primitive::null}};

    std::string id = peek_token().get_lexeme();
    auto it = primitive_map.find(id);
    if (it == primitive_map.end() && peek_token().get_type() != TokenType::tok_identifier) {
        return std::unexpected(
            error(std::format("invalid token found: {}", peek_token().get_lexeme()))
                .with_primary_label(span_from_token(peek_token()), "expected a valid type here")
                .with_help("valid types include: int, float, bool, string, or custom type names")
                .with_note("types must be either primitive types or valid identifiers")
                .with_code("E0030")
                .build());
    }

    advance_token();
    if (it == primitive_map.end()) {
        return Type(id);
    } else {
        return Type(it->second);
    }
}

} // namespace phi
