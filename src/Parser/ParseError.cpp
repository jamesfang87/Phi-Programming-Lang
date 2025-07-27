#include "Parser/Parser.hpp"

namespace phi {
// Emit a "expected X found Y" error
void Parser::emit_expected_found_error(const std::string& expected, const Token& found_token) {
    emit_error(error(std::format("expected {}, found `{}`", expected, found_token.get_lexeme()))
                   .with_primary_label(span_from_token(found_token),
                                       std::format("expected {} here", expected))
                   .build());
}

// Emit an "unexpected token" error with suggestions
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

// Emit an "unclosed delimiter" error with helpful context
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

bool Parser::sync_to() {
    advance_token(); // Skip the problematic token

    while (!at_eof()) {
        // Synchronize on statement boundaries
        switch (peek_token().get_type()) {
            case TokenType::tok_fun:
            case TokenType::tok_class:
            case TokenType::tok_return:
            case TokenType::tok_if:
            case TokenType::tok_while:
            case TokenType::tok_for:
            case TokenType::tok_semicolon: return true;
            default: break;
        }
        advance_token();
    }
    return false;
}

// Synchronize to a specific set of tokens
// Returns true if one of the target tokens was found, false if EOF reached
bool Parser::sync_to(const std::initializer_list<TokenType> target_tokens) {
    while (!at_eof()) {
        for (const TokenType target : target_tokens) {
            if (peek_token().get_type() == target) {
                return true;
            }
        }
        advance_token();
    }
    return false; // Reached EOF without finding target
}

// Synchronize to a specific token type
// Returns true if the target token was found, false if EOF reached
bool Parser::sync_to(const TokenType target_token) {
    while (!at_eof() && peek_token().get_type() != target_token) {
        advance_token();
    }
    return !at_eof();
}

} // namespace phi
