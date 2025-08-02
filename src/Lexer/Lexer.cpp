/**
 * @file Lexer.cpp
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
 * - Comprehensive error reporting using diagnostic system
 */

#include "Lexer/Lexer.hpp"

#include <cstring>
#include <print>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/Token.hpp"

namespace phi {

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
    return {ret, !diagnostic_manager_->has_errors()};
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
    switch (char c = advance_char()) {
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
        case '&':
            if (match_next('&')) {
                return make_token(TokenType::tok_and);
            } else {
                error("unexpected character '&'")
                    .with_primary_label(get_current_span(), "unexpected character")
                    .with_help("use '&&' for logical AND operation")
                    .with_note("single '&' is not supported in this language")
                    .emit(*diagnostic_manager_);
                return make_token(TokenType::tok_error);
            }
        // Handle single | as error or bitwise operator
        case '|':
            if (match_next('|')) {
                return make_token(TokenType::tok_or);
            } else {
                error("unexpected character '|'")
                    .with_primary_label(get_current_span(), "unexpected character")
                    .with_help("use '||' for logical OR operation")
                    .with_note("single '|' is not supported in this language")
                    .emit(*diagnostic_manager_);
                return make_token(TokenType::tok_error);
            }

        case '"': return parse_string();
        case '\'': return parse_char();

        default: {
            if (std::isalpha(c) || c == '_') {
                return parse_identifier_or_kw();
            }
            if (std::isdigit(c)) {
                return parse_number();
            }

            // Handle unknown character with better error message
            std::string char_display;
            if (std::isprint(c)) {
                char_display = "'" + std::string(1, c) + "'";
            } else {
                char_display = "\\x" + std::format("{:02x}", static_cast<unsigned char>(c));
            }

            error("unexpected character " + char_display)
                .with_primary_label(get_current_span(), "unexpected character")
                .with_help("remove this character or use a valid token")
                .with_note("valid characters include letters, digits, operators, and punctuation")
                .emit(*diagnostic_manager_);

            return make_token(TokenType::tok_error);
        }
    }
}

// =============================================================================
// ERROR HANDLING
// =============================================================================

void Lexer::emit_lexer_error(std::string_view message, std::string_view help_message) {
    auto diagnostic = error(std::string(message)).with_primary_label(get_current_span());

    if (!help_message.empty()) {
        diagnostic.with_help(std::string(help_message));
    }

    diagnostic.emit(*diagnostic_manager_);
}

void Lexer::emit_unterminated_string_error(std::string::iterator string_start_pos,
                                           std::string::iterator string_start_line,
                                           int string_start_line_num) {
    // Calculate where we are now (end of file or current position)
    int current_col = static_cast<int>(cur_char - cur_line) + 1;
    SrcLocation eof_start{.path = path, .line = line_num, .col = current_col};
    SrcLocation eof_end{.path = path, .line = line_num, .col = current_col};
    SrcSpan eof_span{eof_start, eof_end};

    // Calculate where the string started (the opening quote) using passed parameters
    int quote_col = static_cast<int>(string_start_pos - string_start_line) + 1;
    SrcLocation quote_start{.path = path, .line = string_start_line_num, .col = quote_col};
    SrcLocation quote_end{.path = path, .line = string_start_line_num, .col = quote_col + 1};
    SrcSpan quote_span{quote_start, quote_end};

    error("unterminated string literal")
        .with_primary_label(quote_span, "string starts here")
        .with_help("add a closing double quote (\") to terminate the string")
        .emit(*diagnostic_manager_);
}

void Lexer::emit_unterminated_char_error() {
    // Find the opening quote position
    auto quote_pos = cur_lexeme;
    int quote_col = static_cast<int>(quote_pos - lexeme_line) + 1;

    SrcLocation quote_start{.path = path, .line = line_num, .col = quote_col};
    SrcLocation quote_end{.path = path, .line = line_num, .col = quote_col + 1};
    SrcSpan quote_span{quote_start, quote_end};

    error("unterminated character literal")
        .with_primary_label(quote_span, "character started here")
        .with_help("add a closing single quote (') to terminate the character")
        .emit(*diagnostic_manager_);
}

void Lexer::emit_unclosed_block_comment_error(std::string::iterator comment_start_pos,
                                              std::string::iterator comment_start_line,
                                              int comment_start_line_num) {
    // Calculate current position (where we reached EOF)
    int current_col = static_cast<int>(cur_char - cur_line) + 1;
    SrcLocation current_start{.path = path, .line = line_num, .col = current_col};
    SrcLocation current_end{.path = path, .line = line_num, .col = current_col};
    SrcSpan current_span{current_start, current_end};

    // Calculate where the block comment started using passed parameters
    int comment_col = static_cast<int>(comment_start_pos - comment_start_line) + 1;
    SrcLocation start_loc{.path = path, .line = comment_start_line_num, .col = comment_col};
    SrcLocation end_loc{.path = path, .line = comment_start_line_num, .col = comment_col + 2};
    SrcSpan comment_span{start_loc, end_loc};

    error("unclosed block comment")
        .with_primary_label(comment_span, "block comment starts here")
        .with_help("add a closing `*/` to terminate the block comment")
        .emit(*diagnostic_manager_);
}

SrcLocation Lexer::get_current_location() const {
    int col = static_cast<int>(cur_char - lexeme_line) + 1;
    return SrcLocation{.path = path, .line = line_num, .col = col};
}

SrcSpan Lexer::get_current_span() const {
    int start_col = static_cast<int>(cur_lexeme - lexeme_line) + 1;
    int end_col = static_cast<int>(cur_char - cur_line) + 1;

    SrcLocation start{.path = path, .line = line_num, .col = start_col};
    SrcLocation end{.path = path, .line = line_num, .col = end_col};

    return SrcSpan{start, end};
}

} // namespace phi
