/**
 * @file scanner.cpp
 * @brief Implementation of the Scanner class for lexical analysis
 *
 * This file contains the complete implementation of the Scanner class,
 * including all token parsing methods, error handling, and helper functions.
 * The scanner processes Phi source code character by character, producing a
 * stream of tokens that can be consumed by the parser.
 *
 * Key features implemented:
 * - Multi-character operator recognition (e.g., ==, +=, ->)
 * - String literal parsing with full escape sequence support
 * - Numeric literal parsing (integers and floats)
 * - Keyword vs identifier distinction
 * - Comment handling (line and block comments)
 * - Comprehensive error reporting with source location
 *
 * Function Organization:
 * 1. Constructor and public interface
 * 2. Main scanning logic
 * 3. Token parsing methods
 * 4. Utility functions
 * 5. Error handling
 *
 *
 * TODO:
 * 1. pass in location to throw_scanning_error
 */

#include "Lexer/Lexer.hpp"
#include "Lexer/token.hpp"
#include <cctype>
#include <cstring>
#include <iostream>
#include <print>
#include <string>
#include <string_view>
#include <unordered_map>
#include <utility>
#include <vector>

// =============================================================================
// PUBLIC INTERFACE IMPLEMENTATION
// =============================================================================

// =============================================================================
// MAIN SCANNING LOGIC
// =============================================================================

/**
 * @brief Main scanning loop that processes the entire source code
 *
 * This is the primary entry point for lexical analysis. It iterates through
 * the source code character by character, handling whitespace, comments, and
 * delegating token recognition to scan_token(). The method maintains proper
 * line and column tracking throughout the scanning process.
 *
 * The scanning process:
 * 1. Skip whitespace (tracking newlines for line numbers)
 * 2. Skip comments (both line comments and block comments)
 * 3. Scan individual tokens using scan_token()
 * 4. Continue until end of file
 *
 * @return A pair containing the vector of tokens and success status
 */
std::pair<std::vector<Token>, bool> Scanner::scan() {
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
    return {ret, successful};
}

/**
 * @brief Scans a single token from the current position
 *
 * This is the main token recognition method that analyzes the current character
 * and determines what type of token to create. It handles:
 *
 * Single-character tokens:
 * - Parentheses, braces, brackets
 * - Basic operators (+, -, *, /, %, !)
 * - Punctuation (comma, semicolon, colon, dot)
 *
 * Multi-character tokens:
 * - Compound operators (==, !=, <=, >=, ++, --, +=, -=, etc.)
 * - Function return arrow (->)
 * - Namespace member (::)
 * - Logical operators (&&, ||)
 *
 * Complex tokens (delegated to specialized methods):
 * - String literals (parse_string)
 * - Character literals (parse_char)
 * - Numeric literals (parse_number)
 * - Identifiers and keywords (parse_identifier)
 *
 * @return A token representing the scanned lexical element
 */
Token Scanner::scan_token() {
    char c = advance_char();
    switch (c) {
        // One char tokens
        case '(': return make_token(TokenType::tok_open_paren);
        case ')': return make_token(TokenType::tok_close_paren);
        case '{': return make_token(TokenType::tok_open_brace);
        case '}': return make_token(TokenType::tok_close_brace);
        case '[': return make_token(TokenType::tok_open_bracket);
        case ']': return make_token(TokenType::tok_close_bracket);
        case ',': return make_token(TokenType::tok_comma);
        case ';': return make_token(TokenType::tok_semicolon);
        case '.': return make_token(TokenType::tok_member);
        case ':':
            return make_token(match_next(':') ? TokenType::tok_namespace_member
                                              : TokenType::tok_colon);

        // Operators
        case '+':
            if (match_next('+')) return make_token(TokenType::tok_increment);
            if (match_next('=')) return make_token(TokenType::tok_plus_equals);
            return make_token(TokenType::tok_add);
        case '-':
            if (match_next('>')) return make_token(TokenType::tok_fun_return);
            if (match_next('-')) return make_token(TokenType::tok_decrement);
            if (match_next('=')) return make_token(TokenType::tok_sub_equals);
            return make_token(TokenType::tok_sub);
        case '*':
            return make_token(match_next('=') ? TokenType::tok_mul_equals : TokenType::tok_mul);
        case '/':
            return make_token(match_next('=') ? TokenType::tok_div_equals : TokenType::tok_div);
        case '%':
            return make_token(match_next('=') ? TokenType::tok_mod_equals : TokenType::tok_mod);
        case '!':
            return make_token(match_next('=') ? TokenType::tok_not_equal : TokenType::tok_bang);
        case '=': return make_token(match_next('=') ? TokenType::tok_equal : TokenType::tok_assign);
        case '<':
            return make_token(match_next('=') ? TokenType::tok_less_equal : TokenType::tok_less);
        case '>':
            return make_token(match_next('=') ? TokenType::tok_greater_equal
                                              : TokenType::tok_greater);
        // Handle single & as error or bitwise operator
        case '&': return make_token(match_next('&') ? TokenType::tok_and : TokenType::tok_error);
        // Handle single | as error or bitwise operator
        case '|': return make_token(match_next('|') ? TokenType::tok_or : TokenType::tok_error);

        case '"': return parse_string();
        case '\'': return parse_char();

        default:
            if (std::isalpha(c) || c == '_') {
                return parse_identifier();
            } else if (std::isdigit(c)) {
                return parse_number();
            } else {
                std::println("Unknow token found: {}\n", c);
                return make_token(TokenType::tok_error);
            }
    }
}

// =============================================================================
// TOKEN PARSING METHODS
// =============================================================================

/**
 * @brief Parses numeric literals (integers and floating-point numbers)
 *
 * This method handles the parsing of numeric literals, automatically detecting
 * whether the number is an integer or floating-point based on the presence of
 * a decimal point. The method supports:
 * - Integer literals: 42, 123, 0
 * - Floating-point literals: 3.14, 0.5, 123.456
 *
 * @note Exponential notation (e.g., 1.23e4) is planned but not yet implemented
 * @return A token of type tok_int_literal or tok_float_literal
 */
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
    if (floating_point) {
        return make_token(TokenType::tok_float_literal);
    }
    return make_token(TokenType::tok_int_literal);
}

/**
 * @brief Parses identifiers and distinguishes them from keywords
 *
 * This method parses sequences of alphanumeric characters and underscores
 * that start with a letter or underscore. It then checks the parsed identifier
 * against a comprehensive keyword table to determine if it should be tokenized
 * as a keyword or as a user-defined identifier.
 *
 * The keyword table includes:
 * - Control flow keywords (if, else, for, while, etc.)
 * - Declaration keywords (let, fun, class, etc.)
 * - Type keywords (i32, str, bool, etc.)
 * - Literal keywords (true, false)
 *
 * @return A token of the appropriate keyword type or tok_identifier
 */
Token Scanner::parse_identifier() {
    while (std::isalnum(peek_char()) || peek_char() == '_') {
        advance_char();
    }
    std::string identifier(cur_lexeme, cur_char);

    static const std::unordered_map<std::string, TokenType> keywords = {
        {"break", TokenType::tok_break},   {"class", TokenType::tok_class},
        {"const", TokenType::tok_const},   {"continue", TokenType::tok_continue},
        {"else", TokenType::tok_else},     {"elif", TokenType::tok_elif},
        {"false", TokenType::tok_false},   {"for", TokenType::tok_for},
        {"fun", TokenType::tok_fun},       {"if", TokenType::tok_if},
        {"import", TokenType::tok_import}, {"in", TokenType::tok_in},
        {"let", TokenType::tok_let},       {"return", TokenType::tok_return},
        {"true", TokenType::tok_true},     {"while", TokenType::tok_while},
        {"i8", TokenType::tok_i8},         {"i16", TokenType::tok_i16},
        {"i32", TokenType::tok_i32},       {"i64", TokenType::tok_i64},
        {"u8", TokenType::tok_u8},         {"u16", TokenType::tok_u16},
        {"u32", TokenType::tok_u32},       {"u64", TokenType::tok_u64},
        {"f32", TokenType::tok_f32},       {"f64", TokenType::tok_f64},
        {"str", TokenType::tok_str},       {"char", TokenType::tok_char}};

    auto it = keywords.find(identifier);
    return (it != keywords.end()) ? make_token(it->second) : make_token(TokenType::tok_identifier);
}

/**
 * @brief Parses string literals with escape sequence support
 *
 * This method parses a complete string literal enclosed in double quotes.
 * It handles escape sequences, multi-line strings, and proper error reporting
 * for unterminated strings. The method tracks line numbers correctly when
 * strings span multiple lines.
 *
 * Features:
 * - Full escape sequence support via parse_escape_sequence()
 * - Multi-line string support
 * - Proper line number tracking
 * - Error reporting for unterminated strings
 *
 * @return A token of type tok_str_literal containing the parsed string content
 */
Token Scanner::parse_string() {
    inside_str = true;
    const int starting_line = line_num;
    std::string str;

    // parse until we see closing double quote
    while (inside_str && !reached_eof() && peek_char() != '"') {
        if (peek_char() == '\\') {
            advance_char(); // consume the forward slash
            str.push_back(parse_escape_sequence());
            continue;
        }
        // increment line number if we see '\n'
        if (peek_char() == '\n') {
            line_num++;
            cur_line = cur_char;
            advance_char();
        } else {
            str.push_back(advance_char());
        }
    }

    if (reached_eof()) {
        // reached eof without finding closing double quote
        throw_scanning_error("untermianted string literal",
                             "expected closing double quote to match this");
        return make_token(TokenType::tok_error);
    }
    advance_char();     // consume closing quote
    inside_str = false; // we are no longer inside str

    SrcLocation start = {.path = this->path,
                         .line = starting_line,
                         .col = static_cast<int>(cur_lexeme - lexeme_line) + 1};

    SrcLocation end = {.path = this->path,
                       .line = line_num,
                       .col = static_cast<int>(cur_char - cur_line) + 1};

    return {start, end, TokenType::tok_str_literal, str};
}

/**
 * @brief Parses character literals with escape sequence support
 *
 * This method parses a complete character literal enclosed in single quotes.
 * It supports both regular characters and escape sequences. The character
 * literal must contain exactly one character (after escape sequence
 * processing).
 *
 * Examples:
 * - 'a' -> character 'a'
 * - '\n' -> newline character
 * - '\x41' -> character 'A'
 *
 * @return A token of type tok_char_literal containing the character value
 * @throws Scanning error for unterminated character literals
 */
Token Scanner::parse_char() {
    // handle case that char literal is empty
    if (peek_char() == '\'') {
        throw_scanning_error("char literal cannot be empty", "expected expression here");
    }

    char c;
    if (peek_char() != '\\') {
        c = advance_char();
    } else {
        advance_char(); // consume the forward slash
        c = parse_escape_sequence();
    }

    if (peek_char() != '\'') {
        std::string error_msg = "char must contain exactly 1 character";
        if (peek_char() == '\0') {
            error_msg = "unterminated character literal";
        }
        throw_scanning_error(error_msg, "expected closing single quote to match this");
        return make_token(TokenType::tok_error);
    }

    advance_char(); // consume closing quote

    SrcLocation start = {.path = this->path,
                         .line = line_num,
                         .col = static_cast<int>(cur_lexeme - lexeme_line) + 1};

    SrcLocation end = {.path = this->path,
                       .line = line_num,
                       .col = static_cast<int>(cur_char - cur_line) + 1};

    return {start, end, TokenType::tok_char_literal, std::string(1, c)};
}

/**
 * @brief Parses escape sequences within string and character literals
 *
 * This method handles the parsing of escape sequences that begin with a
 * backslash. Supported escape sequences include:
 * - \' (single quote)
 * - \" (double quote)
 * - \n (newline)
 * - \t (tab)
 * - \r (carriage return)
 * - \\\ (backslash)
 * - \0 (null character)
 * - \xNN (hexadecimal escape)
 *
 * @return The actual character value represented by the escape sequence
 * @throws Scanning error for invalid escape sequences
 */
char Scanner::parse_escape_sequence() {
    if (reached_eof()) {
        throw_scanning_error("Unfinished escape sequence",
                             "Expected a valid escape sequence character here");
        return '\0';
    }

    char c = advance_char();
    switch (c) {
        case '\'': return '\'';
        case '"': return '\"';
        case 'n': return '\n';
        case 't': return '\t';
        case 'r': return '\r';
        case '\\': return '\\';
        case '0': return '\0';
        case 'x': return parse_hex_escape();
        default:
            throw_scanning_error("Invalid escape sequence: \\" + std::string(1, c), "");
            return c; // Return character as-is for error recovery
    }
}

/**
 * @brief Parses hexadecimal escape sequences (\xNN format)
 *
 * This method parses exactly two hexadecimal digits following \x and converts
 * them to the corresponding character value. For example, \x41 becomes 'A'.
 *
 * @return The character value represented by the hex digits
 * @throws Scanning error if EOF is reached or invalid hex digits are found
 */
char Scanner::parse_hex_escape() {
    if (!isxdigit(peek_char()) || !isxdigit(peek_next())) {
        throw_scanning_error("Incomplete hex escape sequence",
                             "Expected exactly two hexadecimal digits");
        return '\0';
    }

    char hex[3] = {0};
    for (int i = 0; i < 2; i++) {
        if (reached_eof() || !isxdigit(peek_char())) {
            throw_scanning_error("Incomplete hex escape", "");
            return '\0';
        }
        hex[i] = advance_char();
    }
    return static_cast<char>(std::stoi(hex, nullptr, 16));
}

// =============================================================================
// UTILITY FUNCTIONS
// =============================================================================

/**
 * @brief Skips over comment text (both line and block comments)
 *
 * This method handles both types of comments supported by Phi:
 * - Line comments: // (skip until end of line)
 * - Block comments: begin with slash-star, end with star-slash
 *
 * The method properly handles:
 * - Line number tracking within multi-line block comments
 * - Nested comment detection (reports error for unclosed block comments)
 * - EOF handling within comments
 *
 * @throws Scanning error for unclosed block comments
 */
void Scanner::skip_comment() {
    // first consume the first '/' to decide what to do next
    advance_char();
    if (match_next('/')) {
        // skip until we reach the end of line
        while (peek_char() != '\n') {
            advance_char();
        }
    } else if (match_next('*')) {
        int depth = 1; // depth for nested comments
        // skip until we reach a depth of 0
        while (!reached_eof() && depth > 0) {
            if (peek_char() == '/' && peek_next() == '*') {
                // skip / and *
                advance_char();
                advance_char();
                depth++;
            } else if (peek_char() == '*' && peek_next() == '/') {
                // skip * and /
                advance_char();
                advance_char();
                depth--;
            }

            // increment line number if we see '\n'
            if (peek_char() == '\n') {
                line_num++;
                cur_line = cur_char;
            }
            advance_char();
        }

        if (depth > 0) {
            throw_scanning_error("unclosed block comment", "expected closing `*/` to match this");
        }
    }
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

/**
 * @brief Reports a scanning error with formatted output
 *
 * This method handles error reporting by printing a formatted error message
 * that includes the source location, error description, and a visual indicator
 * of where the error occurred in the source code. It automatically sets the
 * successful flag to false to indicate that scanning encountered an error.
 *
 * The error output includes:
 * - Colored error header
 * - File location (path:line:column)
 * - Source line with error highlighted
 * - Contextual help message
 */
void Scanner::throw_scanning_error(std::string_view message, std::string_view expected_message) {
    successful = false;

    // compute column (1-based) where the error begins
    int col = static_cast<int>(cur_lexeme - lexeme_line) + 1;

    // convert line number to string to get its width
    std::string line_no_str = std::to_string(line_num);
    int gutter_width = line_no_str.size() + 2;

    std::println(std::cerr, "\033[31;1;4merror:\033[0m {}", message);
    std::println(std::cerr, "--> {}:{}:{}", path, line_num, col);

    auto line_end = lexeme_line;
    while (line_end < src.end() && *line_end != '\n') {
        line_end++;
    }
    std::string_view line(&*lexeme_line, static_cast<int>(line_end - lexeme_line));

    std::println(std::cerr, "{}|", std::string(gutter_width, ' '));
    std::println(std::cerr, " {} | {}", line_num, line);
    std::println(std::cerr,
                 "{}|{}^ {}\n",
                 std::string(gutter_width, ' '),
                 std::string(col, ' '),
                 expected_message);

    resync_scanner();
}

void Scanner::resync_scanner() {
    // skip until next valid token start
    while (!reached_eof()) {
        // skip to the end of str if we are inside a str
        if (inside_str) {
            if (peek_char() == '"') {
                advance_char();
                inside_str = false;
                return;
            }
            advance_char();
            continue;
        }

        // valid token starters
        if (strchr("(){}[];,.:", peek_char())) break;

        // significant operators
        if (strchr("+-*/%=!<>&|", peek_char())) break;
        advance_char();
    }

    inside_str = false;
}
