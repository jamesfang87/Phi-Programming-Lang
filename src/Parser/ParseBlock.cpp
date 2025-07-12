#include "Parser/Parser.hpp"

std::unique_ptr<Block> Parser::parse_block() {
    // make sure that the block starts with an open brace, exit and emit error
    // otherwise
    if (peek_token().get_type() != TokenType::tok_open_brace) {
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            "Missing open brace",
                            "Expected missing brace here");
        return nullptr;
    }

    // otherwise, parse block. this part also handles the closing brace
    advance_token(); // eat {
    std::vector<std::unique_ptr<Stmt>> stmts;
    while (peek_token().get_type() != TokenType::tok_close_brace) {
        auto type = peek_token().get_type();
        switch (type) {
            case TokenType::tok_eof:
                throw_parsing_error(lines.size(),
                                    lines.back().size(),
                                    "No closing '}' found",
                                    "Expected `}` to close function body, instead reached EOF");
                return nullptr;
            case TokenType::tok_class:
                throw_parsing_error(peek_token().get_start().line,
                                    peek_token().get_start().col,
                                    "Class declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case TokenType::tok_fun_return:
                throw_parsing_error(peek_token().get_start().line,
                                    peek_token().get_start().col,
                                    "Function return not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case TokenType::tok_fun:
                throw_parsing_error(peek_token().get_start().line,
                                    peek_token().get_start().col,
                                    "Function declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            default:
                stmts.push_back(parse_return_stmt());
                // parse_stmt();
        }
    }

    advance_token(); // eat `}`
    return std::make_unique<Block>(std::move(stmts));
}
