#include "token_types.hpp"
#include <any>
#include <string>

#pragma once

class Token {
public:
    Token(int line, TokenType type);
    Token(int line, TokenType type, std::any literal);
    Token(int line, TokenType type, std::string id);

    std::string as_str();

private:
    int line;
    TokenType type;

    std::any literal;
    std::string identifier;
};
