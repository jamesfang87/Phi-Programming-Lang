#include "Parser/Parser.hpp"

std::expected<std::unique_ptr<Block>, Diagnostic> Parser::parse_block() {
    // make sure that the block starts with an open brace, exit and emit error
    // otherwise
    if (peek_token().get_type() != TokenType::tok_open_brace) {
        // throw_parsing_error(peek_token().get_start().line,
        //                     peek_token().get_start().col,
        //                     "Missing open brace",
        //                     "Expected missing brace here");

        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }

    // otherwise, parse block. this part also handles the closing brace
    advance_token(); // eat {
    std::vector<std::unique_ptr<Stmt>> stmts;
    while (peek_token().get_type() != TokenType::tok_close_brace) {
        auto type = peek_token().get_type();
        switch (type) {
            case TokenType::tok_eof:
                // throw_parsing_error(lines.size(),
                //                     lines.back().size(),
                //                     "No closing '}' found",
                //                     "Expected `}` to close function body, instead reached EOF");
                successful = false; // set success flag to false
                return std::unexpected(Diagnostic());
            case TokenType::tok_class:
                // throw_parsing_error(peek_token().get_start().line,
                //                     peek_token().get_start().col,
                //                     "Class declaration not allowed inside function body",
                //                     "Expected this to be a valid token");
                successful = false; // set success flag to false
                return std::unexpected(Diagnostic());
            case TokenType::tok_fun_return:
                // throw_parsing_error(peek_token().get_start().line,
                //                     peek_token().get_start().col,
                //                     "Function return not allowed inside function body",
                //                     "Expected this to be a valid token");
                successful = false; // set success flag to false
                return std::unexpected(Diagnostic());
            case TokenType::tok_fun:
                // throw_parsing_error(peek_token().get_start().line,
                //                     peek_token().get_start().col,
                //                     "Function declaration not allowed inside function body",
                //                     "Expected this to be a valid token");
                successful = false; // set success flag to false
                return std::unexpected(Diagnostic());
            default:
                auto res = parse_stmt();
                if (!res) return std::unexpected(res.error());
                stmts.push_back(std::move(res.value()));
        }
    }

    advance_token(); // eat `}`
    return std::make_unique<Block>(std::move(stmts));
}
