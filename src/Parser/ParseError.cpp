#include "Parser/Parser.hpp"

namespace phi {

/**
 * Emits an "expected X found Y" error message.
 *
 * @param expected Description of the expected token or syntax element
 * @param found_token The actual token encountered in the input stream
 *
 * Constructs a detailed error message showing what was expected versus what was found,
 * and highlights the location of the unexpected token in the source code.
 */
void Parser::emit_expected_found_error(const std::string& expected, const Token& found_token) {
    emit_error(error(std::format("expected {}, found `{}`", expected, found_token.get_lexeme()))
                   .with_primary_label(span_from_token(found_token),
                                       std::format("expected {} here", expected))
                   .build());
}

/**
 * Emits an "unexpected token" error with suggestions for expected tokens.
 *
 * @param token The unexpected token encountered
 * @param expected_tokens List of valid token types expected at this position
 *
 * Generates an error message showing the unexpected token and provides a help message
 * listing valid alternatives when available. The error highlights the token's location
 * and suggests possible corrections.
 */
void Parser::emit_unexpected_token_error(const Token& token,
                                         const std::vector<std::string>& expected_tokens) {
    auto builder = error(std::format("unexpected token `{}`", token.get_lexeme()))
                       .with_primary_label(span_from_token(token), "unexpected token");

    if (!expected_tokens.empty()) {
        std::string suggestion = "expected ";
        for (size_t i = 0; i < expected_tokens.size(); ++i) {
            if (i > 0) {
                suggestion += i == expected_tokens.size() - 1 ? " or " : ", ";
            }
            suggestion += "`" + expected_tokens[i] + "`";
        }
        builder.with_help(suggestion);
    }

    emit_error(std::move(builder).build());
}

/**
 * Emits an "unclosed delimiter" error with contextual guidance.
 *
 * @param opening_token The opening delimiter token (e.g., '{', '(') that wasn't closed
 * @param expected_closing The matching closing delimiter that was expected
 *
 * Creates an error message indicating an unclosed delimiter, highlights the opening delimiter's
 * location, suggests the required closing delimiter, and adds a note about proper delimiter
 * matching.
 */
void Parser::emit_unclosed_delimiter_error(const Token& opening_token,
                                           const std::string& expected_closing) {
    emit_error(
        error("unclosed delimiter")
            .with_primary_label(span_from_token(opening_token),
                                std::format("unclosed `{}`", opening_token.get_lexeme()))
            .with_help(std::format("expected `{}` to close this delimiter", expected_closing))
            .with_note("delimiters must be properly matched")
            .build());
}

/**
 * Synchronizes the parser to the next statement boundary after an error.
 *
 * @return true if synchronized to a statement boundary, false if reached EOF
 *
 * Recovery strategy:
 * 1. Skips the current erroneous token
 * 2. Advances through tokens until encountering:
 *    - Statement starters (function, class, return, if, while, for)
 *    - Statement terminators (semicolon)
 *    - End of file
 * This minimizes cascading errors by resuming at logical statement boundaries.
 */
bool Parser::sync_to() {
    advance_token(); // Skip the problematic token

    while (!at_eof()) {
        // Check for synchronization points
        switch (peek_token().get_type()) {
            case TokenType::tok_fun:       // Function declaration
            case TokenType::tok_class:     // Class definition
            case TokenType::tok_return:    // Return statement
            case TokenType::tok_if:        // If statement
            case TokenType::tok_while:     // While loop
            case TokenType::tok_for:       // For loop
            case TokenType::tok_semicolon: // Statement terminator
                return true;
            default: break;
        }
        advance_token();
    }
    return false;
}

/**
 * Synchronizes the parser to one of specified token types.
 *
 * @param target_tokens List of token types to synchronize to
 * @return true if any target token was found before EOF, false otherwise
 *
 * Advances through the token stream until encountering one of the specified
 * target tokens. Used for context-specific recovery (e.g., block endings).
 */
bool Parser::sync_to(const std::initializer_list<TokenType> target_tokens) {
    while (!at_eof()) {
        for (const TokenType target : target_tokens) {
            if (peek_token().get_type() == target) {
                return true;
            }
        }
        advance_token();
    }
    return false; // Reached EOF without finding targets
}

/**
 * Synchronizes the parser to a specific token type.
 *
 * @param target_token Token type to synchronize to
 * @return true if the target token was found before EOF, false otherwise
 *
 * Efficiently skips tokens until the exact specified token type is found.
 * Useful for recovering from errors where a specific closing token is expected.
 */
bool Parser::sync_to(const TokenType target_token) {
    while (!at_eof() && peek_token().get_type() != target_token) {
        advance_token();
    }
    return !at_eof(); // Found target unless EOF reached
}

} // namespace phi
