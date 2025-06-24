#include "token_types.hpp"
#include <string>

#pragma once

class Token {
public:
    Token(int line, int col, TokenType type, std::string lexeme);

    std::string as_str() const;

private:
    int line, col;
    TokenType type;
    std::string lexeme;
};
