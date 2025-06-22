#include "token.hpp"
#include <format>

Token::Token(int line, TokenType type) {
    this->line = line;
    this->type = type;

    this->identifier = nullptr;
}

Token::Token(int line, TokenType type, std::any literal, std::string id) {
    this->line = line;
    this->type = type;

    this->literal = std::move(literal);
    this->identifier = std::move(id);
}

std::string Token::as_str() {
    std::string literal_str;
    if (literal.has_value()) {
        if (literal.type() == typeid(int)) {
            literal_str = std::to_string(std::any_cast<int>(literal));
        } else if (literal.type() == typeid(double)) {
            literal_str = std::to_string(std::any_cast<double>(literal));
        } else if (literal.type() == typeid(std::string)) {
            literal_str = std::any_cast<std::string>(literal);
        } else {
            literal_str = "<unknown type>";
        }
    } else {
        literal_str = "null";
    }

    return std::format("token type = {} at line={}:  literal='{}', id='{}')",
                       static_cast<int>(type),
                       line,
                       literal_str,
                       identifier);
}
