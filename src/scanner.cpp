#include "scanner.hpp"
#include "token.hpp"
#include "token_types.hpp"
#include <cctype>
#include <cstdio>
#include <format>
#include <iostream>
#include <print>
#include <string>
#include <unordered_map>
#include <utility>
#include <vector>

Scanner::Scanner(std::string src, std::string path) {
    this->src = std::move(src);
    this->path = path;
    this->line_num = 1;
    this->cur_char = this->src.begin();
    this->cur_lexeme = this->src.begin();
    this->cur_line = this->src.begin();
    this->lexeme_line = this->src.begin();
    this->successful = true;
}

/*================== HELPER METHODS =================*/
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

Token Scanner::make_token(TokenType type) {
    return Token(line_num,
                 static_cast<int>(cur_lexeme - lexeme_line) + 1, // 1-indexed column
                 type,
                 std::string(cur_lexeme, cur_char));
}

void Scanner::output_error(const std::string& message, const std::string& expected_message) {
    successful = false;

    // compute column (1-based) where the error begins
    int col = static_cast<int>(cur_lexeme - lexeme_line) + 1;

    // convert line number to string to get its width
    std::string line_no_str = std::to_string(line_num);
    size_t gutter_width = line_no_str.size() + 2;

    std::println(std::cerr, "{}", message);
    std::println(std::cerr, "--> {}:{}:{}", path, line_num, col);

    auto line_end = lexeme_line;
    while (line_end < src.end() && *line_end != '\n') {
        line_end++;
    }
    std::string_view line_sv(&*lexeme_line, static_cast<size_t>(line_end - lexeme_line));

    std::println(std::cerr, "{}|", std::string(gutter_width, ' '));
    std::println(std::cerr, " {} | {}", line_no_str, line_sv);
    std::println(std::cerr, "{}|{}^ {}\n", std::string(gutter_width, ' '), std::string(col, ' '), expected_message);
}

/*===================================================*/

/*=================== ENTRY METHOD ==================*/
std::pair<short, std::vector<Token>> Scanner::scan() {
    std::vector<Token> ret;

    while (!reached_eof()) {
        // make sure that these two it are pointing to the same place
        cur_lexeme = cur_char;
        lexeme_line = cur_line;

        // handle whitespace
        if (std::isspace(peek_char())) {
            if (advance_char() == '\n') {
                line_num++;
                cur_line = cur_char;
            }
            continue;
        }

        // handle comments
        if (peek_char() == '/' && (peek_next() == '/' || peek_next() == '*')) {
            skip_comment();
            continue;
        }

        // finally, scan the token
        ret.push_back(scan_token());
    }
    return {successful, ret};
}

/*============== PARSING SPECIFIC TYPES =============*/
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
    if (floating_point) {
        return make_token(tok_float_literal);
    } else {
        return make_token(tok_int_literal);
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
            case '\"': str.push_back('\"'); break;
            case '\\': str.push_back('\\'); break;
            default: break;
        }
    }

    if (reached_eof()) {
        // reached eof without finding closing double quote
        output_error("\033[31;1;4merror:\033[0m untermianted string literal",
                     "expected closing double quote to match this");
        return make_token(tok_error);
    } else {
        advance_char(); // consume closing quote
        return make_token(tok_str_literal);
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
            case '\"': c = '\"'; break;
            case '\\': c = '\\'; break;
            default: break;
        }
    }

    if (peek_char() != '\'') {
        output_error("\033[31;1;4merror:\033[0m unterminated character literal",
                     "expected closing single quote to match this");
        return make_token(tok_error);
    }
    advance_char(); // consume closing quote
    return make_token(tok_char_literal);
}

Token Scanner::parse_identifier() {
    while (std::isalnum(peek_char()) || peek_char() == '_') {
        advance_char();
    }
    std::string id(cur_lexeme, cur_char);

    static const std::unordered_map<std::string, TokenType> keywords = {
        {"break", tok_break}, {"class", tok_class},   {"const", tok_const},   {"continue", tok_continue},
        {"else", tok_else},   {"elif", tok_elif},     {"false", tok_false},   {"for", tok_for},
        {"fun", tok_fun},     {"if", tok_if},         {"import", tok_import}, {"in", tok_in},
        {"let", tok_let},     {"return", tok_return}, {"true", tok_true},     {"while", tok_while},
        {"i8", tok_i8},       {"i16", tok_i16},       {"i32", tok_i32},       {"i64", tok_i64},
        {"u8", tok_u8},       {"u16", tok_u16},       {"u32", tok_u32},       {"u64", tok_u64},
        {"f32", tok_f32},     {"f64", tok_f64},       {"str", tok_str},       {"char", tok_char}};

    auto it = keywords.find(id);
    return (it != keywords.end()) ? make_token(it->second) : make_token(tok_identifier);
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
                cur_line = cur_char;
            }
            advance_char();
        }

        output_error("\033[31;1;4merror:\033[0m unclosed block comment", "expected closing `*/` to match this");
    }
}
/*===================================================*/

Token Scanner::scan_token() {
    char c = advance_char();
    switch (c) {
        // One char tokens
        case '(': return make_token(tok_open_paren);
        case ')': return make_token(tok_close_paren);
        case '{': return make_token(tok_open_brace);
        case '}': return make_token(tok_close_brace);
        case '[': return make_token(tok_open_bracket);
        case ']': return make_token(tok_close_bracket);
        case ',': return make_token(tok_comma);
        case '.': return make_token(tok_member);
        case ':': return make_token(match_next(':') ? tok_namespace_member : tok_colon);

        // Operators
        case '+':
            if (match_next('+')) return make_token(tok_increment);
            if (match_next('=')) return make_token(tok_plus_equals);
            return make_token(tok_add);
        case '-':
            if (match_next('>')) return make_token(tok_fun_return);
            if (match_next('-')) return make_token(tok_decrement);
            if (match_next('=')) return make_token(tok_sub_equals);
            return make_token(tok_sub);
        case '*': return make_token(match_next('=') ? tok_mul_equals : tok_mul);
        case '/': return make_token(match_next('=') ? tok_div_equals : tok_div);
        case '%': return make_token(match_next('=') ? tok_mod_equals : tok_mod);
        case '!': return make_token(match_next('=') ? tok_not_equal : tok_bang);
        case '=': return make_token(match_next('=') ? tok_equal : tok_assign);
        case '<': return make_token(match_next('=') ? tok_less_equal : tok_less);
        case '>': return make_token(match_next('=') ? tok_greater_equal : tok_greater);
        // Handle single & as error or bitwise operator
        case '&': return make_token(match_next('&') ? tok_and : tok_error);
        // Handle single | as error or bitwise operator
        case '|': return make_token(match_next('|') ? tok_or : tok_error);

        case '"': return parse_string();
        case '\'': return parse_char();

        default:
            if (isalpha(c) || c == '_')
                return parse_identifier();
            else if (isdigit(c))
                return parse_number();
            else {
                std::println("Unknow token found: {}\n", c);
                return make_token(tok_error);
            }
    }
}
