#include "token.hpp"
#include <string>
#include <vector>

#pragma once
class Scanner {
public:
    Scanner(std::string src);
    std::vector<Token> scan();

private:
    std::string src;

    int line_num;
    std::string::iterator cur_char, cur_lexeme;

    bool reached_eof() const;
    char peek_char() const;
    char peek_next() const;
    char advance_char();
    bool match_next(char next);

    Token parse_number();
    Token parse_string();
    Token parse_char();
    Token parse_identifier();
    void skip_comment();
    Token scan_token();
};
