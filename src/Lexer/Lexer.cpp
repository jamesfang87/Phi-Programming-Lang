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
 * TODO:
 * 1. pass in location to throw_scanning_error
 */

#include "Lexer/Lexer.hpp"
#include "Lexer/token.hpp"
#include <cstring>
#include <iostream>
#include <print>

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
std::pair<std::vector<Token>, bool> Lexer::scan() {
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
Token Lexer::scan_token() {
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

        case '.':
            if (match_next_n(".=")) return make_token(TokenType::tok_exclusive_range);
            if (match_next('.')) return make_token(TokenType::tok_inclusive_range);
            return make_token(TokenType::tok_member);
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
                std::println("Unknown token found: {}\n", c);
                return make_token(TokenType::tok_error);
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
void Lexer::throw_lexer_error(std::string_view message, std::string_view expected_message) {
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

void Lexer::resync_scanner() {
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
