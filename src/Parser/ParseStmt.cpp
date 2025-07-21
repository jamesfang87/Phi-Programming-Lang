#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <print>
#include <vector>

#include "AST/Stmt.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenType.hpp"

std::expected<std::unique_ptr<Stmt>, Diagnostic> Parser::parse_stmt() {
    std::println("{}", peek_token().get_name());
    switch (peek_token().get_type()) {
        case TokenType::tok_return: {
            auto res = parse_return_stmt();
            if (!res) return std::unexpected(res.error());
            return res;
        }
        case TokenType::tok_while: return parse_while_stmt().value();
        case TokenType::tok_if: return parse_if_stmt().value();
        case TokenType::tok_for: return parse_for_stmt().value();
        case TokenType::tok_let: {
            auto res = parse_var_decl();
            if (!res) {
                std::println("error here");
                return std::unexpected(res.error());
            }
            return res;
        }
        default: advance_token();
    }
    return std::unexpected(Diagnostic());
}

std::expected<std::unique_ptr<ReturnStmt>, Diagnostic> Parser::parse_return_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'return'

    // case that there is no expression after 'return'
    // (functions which return null)
    if (peek_token().get_type() == TokenType::tok_semicolon) {
        advance_token(); // eat ';'
        return std::make_unique<ReturnStmt>(loc, nullptr);
    }

    // functions which have a non-void return type
    auto expr = parse_expr();
    if (!expr) {
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
        /*
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            "Invalid expression found",
                            "Expected an expression after `return`");
        return nullptr;
        */
    }

    // check that the line ends with a semicolon
    if (peek_token().get_type() != TokenType::tok_semicolon) {
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
        // throw_parsing_error(peek_token().get_start().line,
        //                     peek_token().get_start().col,
        //                     "Invalid token found",
        //                     "Expected `;` after expression");
        // return nullptr;
    }
    advance_token();

    return std::make_unique<ReturnStmt>(loc, std::move(expr.value()));
}

std::expected<std::unique_ptr<IfStmt>, Diagnostic> Parser::parse_if_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'if'

    auto condition = parse_expr();
    if (!condition) return std::unexpected(condition.error());

    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    if (peek_token().get_type() != TokenType::tok_else) {
        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        nullptr);
    }

    // otherwise, we expect an 'else' keyword
    advance_token(); // eat 'else'

    if (peek_token().get_type() == TokenType::tok_open_brace) {
        // parse the body of the else statement
        auto else_body = parse_block();
        if (!else_body) return std::unexpected(else_body.error());

        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        std::move(else_body.value()));
    } else if (peek_token().get_type() == TokenType::tok_if) {
        std::vector<std::unique_ptr<Stmt>> elif_stmt;

        auto res = parse_if_stmt();
        if (!res) return std::unexpected(res.error());

        // otherwise emplace back and return a valid IfStmt
        elif_stmt.emplace_back(std::move(res.value()));
        std::unique_ptr<Block> elif_body = std::make_unique<Block>(std::move(elif_stmt));
        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        std::move(elif_body));
    }
    successful = false; // set success flag to false
    return std::unexpected(Diagnostic());
}

std::expected<std::unique_ptr<WhileStmt>, Diagnostic> Parser::parse_while_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'while'

    auto condition = parse_expr();
    if (!condition) return std::unexpected(condition.error());

    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    return std::make_unique<WhileStmt>(loc, std::move(condition.value()), std::move(body.value()));
}

// ex: for i in [1, 10)
std::expected<std::unique_ptr<ForStmt>, Diagnostic> Parser::parse_for_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'for'

    // now we expect the looping variable name
    Token looping_var = advance_token();
    if (looping_var.get_type() != TokenType::tok_identifier) {
        // throw_parsing_error(looping_var.get_start().line,
        //                     looping_var.get_start().col,
        //                     "Invalid token found",
        //                     "Expected identifier for looping variable");
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }

    // now we expect the 'in' keyword
    Token in_keyword = advance_token();
    if (in_keyword.get_type() != TokenType::tok_in) {
        // throw_parsing_error(in_keyword.get_start().line,
        //                     in_keyword.get_start().col,
        //                     "Invalid token found",
        //                     "Expected 'in' keyword");
        // return nullptr;
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }

    // now we parse the range literal
    auto range = parse_expr();
    if (!range) return std::unexpected(range.error());

    // parse the body of the for loop
    auto body = parse_block();
    if (!body) return std::unexpected(range.error());

    return std::make_unique<ForStmt>(loc,
                                     looping_var.get_lexeme(),
                                     std::move(range.value()),
                                     std::move(body.value()));
}
