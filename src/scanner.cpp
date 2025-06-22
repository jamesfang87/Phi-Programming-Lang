#include "scanner.hpp"
#include "token.hpp"
#include <cctype>
#include <iostream>
#include <string>
#include <vector>

Scanner::Scanner(std::string src) {
    this->src = std::move(src);
    this->line_num = 1;
    this->cur_char = this->src.begin();
    this->cur_lexeme = this->src.begin();
}

/*============= HELPER METHODS ============*/
bool Scanner::reached_eof() const {
    return cur_char >= src.end();
}

char Scanner::peek_char() const {
    return (reached_eof()) ? '\0' : *cur_char;
}

char Scanner::peek_next() const {
    if (reached_eof() || cur_char + 1 >= src.end()) return '\0';
    return *(cur_char + 1);
}

char Scanner::advance_char() {
    if (reached_eof()) return '\0';
    return *cur_char++;
}

bool Scanner::match_next(char next) {
    if (reached_eof() || peek_char() != next) {
        return false;
    }

    cur_char++;
    return true;
}
/*=========================================*/

/*============== ENTRY METHOD =============*/
std::vector<Token> Scanner::scan() {
    std::vector<Token> ret;

    while (!reached_eof()) {
        // make sure that these two it are pointing to the same place
        cur_lexeme = cur_char;

        // handle whitespace
        if (isspace(peek_char())) {
            if (advance_char() == '\n') {
                line_num++;
            }
            continue;
        }

        // handle comments
        if (peek_char() == '/' && (peek_next() == '/' || peek_next() == '*')) {
            skip_comment();
        }

        // finally, scan the token
        ret.push_back(scan_token());
    }
    return ret;
}

/*=========================================*/
Token Scanner::parse_number() {
    bool floating_point = false;

    // integer part
    while (std::isdigit(peek_char())) {
        advance_char();
    }

    // fractional part
    if (peek_char() == '.') {
        floating_point = true;
        advance_char(); // consume '.'
        while (std::isdigit(peek_char())) {
            advance_char();
        }
    }

    // TODO: implement exponents
    std::string num_str(cur_lexeme, cur_char);
    try {
        if (floating_point) {
            return Token(line_num, tok_float_literal, std::stod(num_str), num_str);
        } else {
            return Token(line_num, tok_int_literal, std::stoll(num_str), num_str);
        }
    } catch (const std::exception&) {
        std::cout << "Error processing number: " + num_str + '\n';
        return Token(line_num, tok_error);
    }
}

Token Scanner::parse_string() {
    std::string str;

    // parse until we see closing double quote
    while (!reached_eof() && peek_char() != '"') {
        if (peek_char() != '\\') {
            str.push_back(advance_char());
            continue;
        }

        switch (advance_char()) {
            case 'n': str.push_back('\n'); break;
            case 't': str.push_back('\t'); break;
            case 'r': str.push_back('\r'); break;
            case '\'': str.push_back('\''); break;
            case '\\': str.push_back('\\'); break;
            default: break;
        }
    }

    if (reached_eof()) {
        // reached eof without finding closing double quote
        std::cout << "Did not find closing double quotes\n";
        return Token(line_num, tok_error);
    } else {
        return Token(line_num, tok_str_literal, str, "");
    }
}

Token Scanner::parse_char() {
    char c;
    if (peek_char() != '\\') {
        c = advance_char();
    } else {
        advance_char(); // consume the forward slash
        switch (advance_char()) {
            case 'n': c = '\n'; break;
            case 't': c = '\t'; break;
            case 'r': c = '\r'; break;
            case '\'': c = '\''; break;
            case '\\': c = '\\'; break;
            default: break;
        }
    }

    if (peek_char() != '\'') {
        std::cout << "Unterminated character literal\n";
        return Token(line_num, tok_error);
    }
    return Token(line_num, tok_char_literal, c, "");
}

void Scanner::skip_comment() {
    // first consume the first '/' to decide what to do next
    advance_char();
    if (match_next('/')) {
        // skip until we reach the end of line
        while (peek_char() != '\n') {
            advance_char();
        }
    } else if (match_next('*')) {
        // skip until we reach the next "*/"
        while (!reached_eof()) {
            if (peek_char() == '*' && peek_next() == '/') {
                // skip * and /
                advance_char();
                advance_char();
                return;
            }

            // increment line number if we see '\n'
            if (peek_char() == '\n') {
                line_num++;
            }
            advance_char();
        }
        std::cout << "Unclosed block comment\n";
    }
}

Token Scanner::scan_token() {
    switch (advance_char()) {
        // One char tokens
        case '(': return Token(line_num, tok_open_paren);
        case ')': return Token(line_num, tok_close_paren);
        case '{': return Token(line_num, tok_open_brace);
        case '}': return Token(line_num, tok_close_brace);
        case '[': return Token(line_num, tok_open_bracket);
        case ']': return Token(line_num, tok_close_bracket);
        case '.': return Token(line_num, tok_member);
        case ':': return Token(line_num, tok_colon);

        // Operators
        case '+': return Token(line_num, match_next('+') ? tok_increment : tok_add);
        case '-':
            if (match_next('>')) return Token(line_num, tok_fun_return);
            return Token(line_num, match_next('-') ? tok_decrement : tok_sub);
        case '*': return Token(line_num, tok_mul);
        case '/': return Token(line_num, tok_div);
        case '!': return Token(line_num, match_next('=') ? tok_not_equal : tok_bang);
        case '=': return Token(line_num, match_next('=') ? tok_equal : tok_assign);
        case '<': return Token(line_num, match_next('=') ? tok_less_equal : tok_less);
        case '>': return Token(line_num, match_next('=') ? tok_greater_equal : tok_greater);

        case '"': parse_string();
        case '\'': parse_char();

        default: std::cout << "Unknown token found\n"; return Token(line_num, tok_error);
    }
}
