#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <vector>

#include "AST/Stmt.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

std::expected<std::unique_ptr<Stmt>, Diagnostic> Parser::parse_stmt() {
    switch (peek_token().get_type()) {
        case TokenType::tok_return: return parse_return_stmt();
        case TokenType::tok_while: return parse_while_stmt();
        case TokenType::tok_if: return parse_if_stmt().value();
        case TokenType::tok_for: return parse_for_stmt().value();
        case TokenType::tok_let: {
            auto res = parse_let_stmt();
            if (!res) return std::unexpected(res.error());
            return res;
        }
        default: advance_token();
    }
    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
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
        emit_error(error("invalid return expression")
                       .with_primary_label(span_from_token(peek_token()), "invalid expression here")
                       .with_help("return statements can be `return;` or `return expression;`")
                       .with_code("E0011")
                       .build());
        return std::unexpected(expr.error());
    }

    // check that the line ends with a semicolon
    if (peek_token().get_type() != TokenType::tok_semicolon) {
        emit_error(error("missing semicolon after return statement")
                       .with_primary_label(span_from_token(peek_token()), "expected `;` here")
                       .with_help("return statements must end with a semicolon")
                       .with_suggestion(span_from_token(peek_token()), ";", "add semicolon")
                       .with_code("E0012")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing semicolon"));
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
    }
    if (peek_token().get_type() == TokenType::tok_if) {
        std::vector<std::unique_ptr<Stmt>> elif_stmt;

        auto res = parse_if_stmt();
        if (!res) return std::unexpected(res.error());

        // otherwise emplace back and return a valid IfStmt
        elif_stmt.emplace_back(std::move(res.value()));
        auto elif_body = std::make_unique<Block>(std::move(elif_stmt));
        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        std::move(elif_body));
    }
    emit_error(error("invalid else clause")
                   .with_primary_label(span_from_token(peek_token()), "unexpected token here")
                   .with_help("else must be followed by a block `{` or another `if` statement")
                   .with_code("E0040")
                   .build());
    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid else clause"));
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

std::expected<std::unique_ptr<ForStmt>, Diagnostic> Parser::parse_for_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'for'

    // now we expect the looping variable name
    Token looping_var = advance_token();
    if (looping_var.get_type() != TokenType::tok_identifier) {
        emit_error(error("for loop must have a loop variable")
                       .with_primary_label(span_from_token(looping_var), "expected identifier here")
                       .with_help("for loops have the form: `for variable in iterable`")
                       .with_note("the loop variable will be assigned each value from the iterable")
                       .with_code("E0014")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid for loop"));
    }

    // now we expect the 'in' keyword
    Token in_keyword = advance_token();
    if (in_keyword.get_type() != TokenType::tok_in) {
        emit_error(error("missing `in` keyword in for loop")
                       .with_primary_label(span_from_token(looping_var), "loop variable")
                       .with_secondary_label(span_from_token(in_keyword), "expected `in` here")
                       .with_help("for loops have the form: `for variable in iterable`")
                       .with_suggestion(span_from_token(in_keyword), "in", "add `in` keyword")
                       .with_code("E0015")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing in keyword"));
    }

    // now we parse the range literal
    auto range = parse_expr();
    if (!range) return std::unexpected(range.error());

    // parse the body of the for loop
    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    auto loop_var = make_unique<VarDecl>(looping_var.get_start(),
                                         looping_var.get_lexeme(),
                                         Type(Type::Primitive::i64),
                                         false,
                                         nullptr);

    return std::make_unique<ForStmt>(loc,
                                     std::move(loop_var),
                                     std::move(range.value()),
                                     std::move(body.value()));
}

std::expected<std::unique_ptr<LetStmt>, Diagnostic> Parser::parse_let_stmt() {
    SrcLocation loc = peek_token().get_start();
    if (advance_token().get_type() != TokenType::tok_let) {
        emit_unexpected_token_error(peek_token(), {"let"});
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
    }

    auto binding = parse_typed_binding();
    if (!binding) {
        return std::unexpected(binding.error());
    }

    std::string name = binding.value().first;
    Type type = std::move(binding.value().second);

    // now we expect a `=` token
    if (advance_token().get_type() != TokenType::tok_assign) {
        emit_error(error("missing assignment in variable declaration")
                       .with_primary_label(span_from_token(peek_token()), "expected `=` here")
                       .with_help("variables must be initialized with a value")
                       .with_note("variable syntax: `let name: type = value;`")
                       .with_code("E0023")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing assignment"));
    }

    // lastly, we expect an expression
    auto expr = parse_expr();
    if (!expr) {
        emit_error(
            error("variable declaration must have an initializer expression")
                .with_primary_label(span_from_token(peek_token()), "expected expression here")
                .with_help("provide an expression to initialize the variable")
                .with_code("E0024")
                .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing initializer"));
    }

    // now we expect a `;` token
    if (advance_token().get_type() != TokenType::tok_semicolon) {
        emit_error(error("missing semicolon after variable declaration")
                       .with_primary_label(span_from_token(peek_token()), "expected `;` here")
                       .with_help("variable declarations must end with a semicolon")
                       .with_suggestion(span_from_token(peek_token()), ";", "add semicolon")
                       .with_code("E0025")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing semicolon"));
    }

    return std::make_unique<LetStmt>(
        loc,
        std::make_unique<VarDecl>(loc, name, type, true, std::move(expr.value())));
}

} // namespace phi
