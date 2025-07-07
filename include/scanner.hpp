#include "token.hpp"
#include <string>
#include <vector>

#pragma once

class Scanner {
public:
    Scanner(std::string src, std::string path);
    std::pair<bool, std::vector<Token>> scan();

    std::string get_src() { return src; }
    std::string get_path() { return path; }

private:
    std::string src;
    std::string path;

    int line_num;
    std::string::iterator cur_char, cur_lexeme, cur_line, lexeme_line;

    bool successful;

    void report_error(const std::string& message, const std::string& expected_message);
    [[nodiscard]] bool reached_eof() const;
    [[nodiscard]] char peek_char() const;
    [[nodiscard]] char peek_next() const;
    char advance_char();
    bool match_next(char next);
    Token make_token(TokenType type);

    Token scan_token();
    Token parse_number();
    Token parse_string();
    Token parse_char();
    char parse_escape_sequence();
    char parse_hex_escape();
    Token parse_identifier();
    void skip_comment();
};
