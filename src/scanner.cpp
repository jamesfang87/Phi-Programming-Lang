/**
 * @file scanner.cpp
 * @brief Implementation of the Scanner class for lexical analysis
 * 
 * This file contains the complete implementation of the Scanner class, including
 * all token parsing methods, error handling, and helper functions. The scanner
 * processes Phi source code character by character, producing a stream of tokens
 * that can be consumed by the parser.
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
 */

#include "scanner.hpp"
#include "token.hpp"
#include <cctype>
#include <cstdio>
#include <iostream>
#include <print>
#include <string>
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
        case '(': return make_token(tok_open_paren);
        case ')': return make_token(tok_close_paren);
        case '{': return make_token(tok_open_brace);
        case '}': return make_token(tok_close_brace);
        case '[': return make_token(tok_open_bracket);
        case ']': return make_token(tok_close_bracket);
        case ',': return make_token(tok_comma);
        case ';': return make_token(tok_semicolon);
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
            if (std::isalpha(c) || c == '_') {
                return parse_identifier();
            } else if (std::isdigit(c)) {
                return parse_number();
            } else {
                std::println("Unknow token found: {}\n", c);
                return make_token(tok_error);
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
        return make_token(tok_float_literal);
    }
    return make_token(tok_int_literal);
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
        {"break", tok_break},   {"class", tok_class},
        {"const", tok_const},   {"continue", tok_continue},
        {"else", tok_else},     {"elif", tok_elif},
        {"false", tok_false},   {"for", tok_for},
        {"fun", tok_fun},       {"if", tok_if},
        {"import", tok_import}, {"in", tok_in},
        {"let", tok_let},       {"return", tok_return},
        {"true", tok_true},     {"while", tok_while},
        {"i8", tok_i8},         {"i16", tok_i16},
        {"i32", tok_i32},       {"i64", tok_i64},
        {"u8", tok_u8},         {"u16", tok_u16},
        {"u32", tok_u32},       {"u64", tok_u64},
        {"f32", tok_f32},       {"f64", tok_f64},
        {"str", tok_str},       {"char", tok_char}};

    auto it = keywords.find(identifier);
    return (it != keywords.end()) ? make_token(it->second) : make_token(tok_identifier);
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
    const int starting_line = line_num;
    std::string str;

    // parse until we see closing double quote
    while (!reached_eof() && peek_char() != '"') {
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
        throw_scanning_error("untermianted string literal", "expected closing double quote to match this");
        return make_token(tok_error);
    }
    advance_char(); // consume closing quote
    return {starting_line, static_cast<int>(cur_lexeme - lexeme_line) + 1, tok_str_literal, str};
}

/**
 * @brief Parses character literals with escape sequence support
 * 
 * This method parses a complete character literal enclosed in single quotes.
 * It supports both regular characters and escape sequences. The character
 * literal must contain exactly one character (after escape sequence processing).
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
    char c;
    if (peek_char() != '\\') {
        c = advance_char();
    } else {
        advance_char(); // consume the forward slash
        c = parse_escape_sequence();
    }

    if (peek_char() != '\'') {
        throw_scanning_error("unterminated character literal",
                     "expected closing single quote to match this");
        return make_token(tok_error);
    }
    advance_char(); // consume closing quote
    return {line_num,
            static_cast<int>(cur_lexeme - lexeme_line) + 1,
            tok_char_literal,
            std::string{c}};
}

/**
 * @brief Parses escape sequences within string and character literals
 * 
 * This method handles the parsing of escape sequences that begin with a backslash.
 * Supported escape sequences include:
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

        throw_scanning_error("unclosed block comment", "expected closing `*/` to match this");
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
void Scanner::throw_scanning_error(const std::string& message, const std::string& expected_message) {
    successful = false;

    // compute column (1-based) where the error begins
    int col = static_cast<int>(cur_lexeme - lexeme_line) + 1;

    // convert line number to string to get its width
    std::string line_no_str = std::to_string(line_num);
    size_t gutter_width = line_no_str.size() + 2;

    std::println(std::cerr, "\033[31;1;4merror:\033[0m {}", message);
    std::println(std::cerr, "--> {}:{}:{}", path, line_num, col);

    auto line_end = lexeme_line;
    while (line_end < src.end() && *line_end != '\n') {
        line_end++;
    }
    std::string_view line(&*lexeme_line, static_cast<size_t>(line_end - lexeme_line));

    std::println(std::cerr, "{}|", std::string(gutter_width, ' '));
    std::println(std::cerr, " {} | {}", line_num, line);
    std::println(std::cerr,
                 "{}|{}^ {}\n",
                 std::string(gutter_width, ' '),
                 std::string(col, ' '),
                 expected_message);
}
