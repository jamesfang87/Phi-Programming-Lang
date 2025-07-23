#include "Parser/Parser.hpp"

namespace phi {

std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> Parser::parse() {
    while (!at_eof() && peek_token().get_type() != TokenType::tok_eof) {
        switch (peek_token().get_type()) {
            case TokenType::tok_fun: {
                auto res = parse_function_decl();
                if (res.has_value()) {
                    functions.push_back(std::move(res.value()));
                } else {
                    // Error was already emitted, try to synchronize and continue
                    synchronize();
                }
                break;
            }
            case TokenType::tok_eof: return {std::move(functions), successful};
            default:
                emit_unexpected_token_error(peek_token(), {"fun"});
                synchronize();
                break;
        }
    }

    return {std::move(functions), successful};
}

bool Parser::expect_token(TokenType expected_type, const std::string& context) {
    if (peek_token().get_type() == expected_type) {
        advance_token();
        return true;
    }

    std::string context_msg = context.empty() ? "" : " in " + context;
    emit_expected_found_error(type_to_string(expected_type) + context_msg, peek_token());
    return false;
}

bool Parser::match_token(TokenType type) {
    if (peek_token().get_type() == type) {
        advance_token();
        return true;
    }
    return false;
}

void Parser::skip_until_recovery_point() {
    while (!at_eof()) {
        switch (peek_token().get_type()) {
            case TokenType::tok_semicolon:
            case TokenType::tok_close_brace:
            case TokenType::tok_fun: return;
            default: advance_token(); break;
        }
    }
}

bool Parser::is_statement_start() const {
    switch (peek_token().get_type()) {
        case TokenType::tok_return:
        case TokenType::tok_if:
        case TokenType::tok_while:
        case TokenType::tok_for:
        case TokenType::tok_identifier:
        case TokenType::tok_open_brace: return true;
        default: return false;
    }
}

bool Parser::is_expression_start() const {
    switch (peek_token().get_type()) {
        case TokenType::tok_identifier:
        case TokenType::tok_int_literal:
        case TokenType::tok_float_literal:
        case TokenType::tok_str_literal:
        case TokenType::tok_open_paren: return true;
        default: return false;
    }
}
} // namespace phi
