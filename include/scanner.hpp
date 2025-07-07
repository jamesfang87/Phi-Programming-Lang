#include "token.hpp"
#include <string>
#include <vector>

#pragma once

class Scanner {
public:
    Scanner(std::string src, std::string path);
    std::pair<std::vector<Token>, bool> scan();

    std::string get_src() { return src; }
    std::string get_path() { return path; }

private:
    std::string src;
    std::string path;

    int line_num;
    std::string::iterator cur_char, cur_lexeme, cur_line, lexeme_line;

    bool successful;

    void throw_scanning_error(const std::string& message, const std::string& expected_message);
    
    /// Returns true if reached eof, false otherwise
    [[nodiscard]] inline bool reached_eof() const { return cur_char >= src.end(); }
    
    /// Returns the value of the current char the scanner is pointing to
    /// If we have reached the eof, then returns '\0'
    [[nodiscard]] inline char peek_char() const { return (reached_eof()) ? '\0' : *cur_char; }
    
    /// Returns the value of the next char.
    /// If this char is past eof, then returns '\0'
    [[nodiscard]] inline char peek_next() const {
        if (reached_eof() || cur_char + 1 >= src.end()) {
            return '\0';
        }
        return *(cur_char + 1);
    }
    
    /// Advances the char the scanner is pointing to by one
    /// Returns the value of the former char, '\0' if eof
    inline char advance_char() {
        if (reached_eof()) return '\0';
        return *cur_char++;
    }
    
    /// Returns true if the next char matches parameter 'next', false otherwise.
    /// If the next character matches, then this advances the char the scanner points to
    inline bool match_next(char next) {
        if (reached_eof() || peek_char() != next) {
            return false;
        }
        cur_char++;
        return true;
    }
    
    /// Returns a new Token given the type of token
    inline Token make_token(TokenType type) {
        return {line_num,
                static_cast<int>(cur_lexeme - lexeme_line) + 1, // 1-indexed column
                type,
                std::string(cur_lexeme, cur_char)};
    }

    Token scan_token();
    Token parse_number();
    Token parse_string();
    Token parse_char();
    char parse_escape_sequence();
    char parse_hex_escape();
    Token parse_identifier();
    void skip_comment();
};
