#include "Parser/Parser.hpp"

namespace phi {

bool Parser::at_eof() const {
    return token_it >= tokens.end() || peek_token().get_type() == TokenType::tok_eof;
}

Token Parser::peek_token() const {
    if (token_it >= tokens.end()) {
        // return a synthetic EOF token
        const SrcLocation eof_loc{.path = "", .line = -1, .col = -1};
        return Token{eof_loc, eof_loc, TokenType::tok_eof, ""};
    }
    return *token_it;
}

Token Parser::advance_token() {
    Token ret = peek_token();
    if (token_it < tokens.end()) {
        ++token_it;
    }
    return ret;
}

bool Parser::expect_token(const TokenType expected_type, const std::string& context) {
    if (peek_token().get_type() == expected_type) {
        advance_token();
        return true;
    }

    const std::string context_msg = context.empty() ? "" : " in " + context;
    emit_expected_found_error(type_to_string(expected_type) + context_msg, peek_token());
    return false;
}

bool Parser::match_token(const TokenType type) {
    if (peek_token().get_type() == type) {
        advance_token();
        return true;
    }
    return false;
}

} // namespace phi
