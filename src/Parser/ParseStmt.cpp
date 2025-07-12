#include "Parser/Parser.hpp"

std::unique_ptr<Stmt> Parser::parse_stmt() {
    advance_token();
    if (peek_token().get_type() == TokenType::tok_return) {
        return parse_return_stmt();
    }

    // TODO: Handle other statement types
    return nullptr;
}

std::unique_ptr<ReturnStmt> Parser::parse_return_stmt() {
    int line = peek_token().get_start().line, col = peek_token().get_start().col;
    advance_token(); // eat 'return'

    // case that there is no expression after 'return'
    // (functions which return null)
    if (peek_token().get_type() == TokenType::tok_semicolon) {
        advance_token(); // eat ';'
        return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                            nullptr);
    }

    // functions which have a non-void return type
    std::unique_ptr<Expr> expr = parse_expr();
    if (expr == nullptr) {
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            "Invalid expression found",
                            "Expected an expression after `return`");
        return nullptr;
    }

    // check that the line ends with a semicolon
    if (peek_token().get_type() != TokenType::tok_semicolon) {
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            "Invalid token found",
                            "Expected `;` after expression");
        return nullptr;
    }
    advance_token();

    return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                        std::move(expr));
}
