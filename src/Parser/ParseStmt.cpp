#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <vector>

#include "AST/Stmt.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Dispatches to specific statement parsers based on current token.
 *
 * @return std::expected<std::unique_ptr<Stmt>, Diagnostic> Statement AST or error.
 *
 * Handles:
 * - Return statements
 * - While loops
 * - If statements
 * - For loops
 * - Variable declarations (let)
 */
std::expected<std::unique_ptr<Stmt>, Diagnostic> Parser::parse_stmt() {
    switch (peek_token().get_type()) {
        case TokenType::tok_return: return parse_return_stmt();
        case TokenType::tok_if: return parse_if_stmt();
        case TokenType::tok_while: return parse_while_stmt();
        case TokenType::tok_for: return parse_for_stmt();
        case TokenType::tok_let: return parse_let_stmt();
        default: return parse_expr(); // attempt to parse as expr
    }
    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
}

/**
 * Parses a return statement.
 *
 * @return std::expected<std::unique_ptr<ReturnStmt>, Diagnostic> Return AST or error.
 *
 * Formats:
 * - return;       (implicit null)
 * - return expr;  (explicit value)
 *
 * Validates semicolon terminator and expression validity.
 */
std::expected<std::unique_ptr<ReturnStmt>, Diagnostic> Parser::parse_return_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'return'

    // Null return: return;
    if (peek_token().get_type() == TokenType::tok_semicolon) {
        advance_token(); // eat ';'
        return std::make_unique<ReturnStmt>(loc, nullptr);
    }

    // Value return: return expr;
    auto expr = parse_expr();
    if (!expr) {
        emit_error(error("invalid return expression")
                       .with_primary_label(span_from_token(peek_token()), "invalid expression here")
                       .with_help("return statements can be `return;` or `return expression;`")
                       .with_code("E0011")
                       .build());
        return std::unexpected(expr.error());
    }

    // Validate semicolon terminator
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

/**
 * Parses an if statement with optional else clause.
 *
 * @return std::expected<std::unique_ptr<IfStmt>, Diagnostic> If statement AST or error.
 *
 * Handles:
 * - if (cond) { ... }
 * - if (cond) { ... } else { ... }
 * - if (cond) { ... } else if { ... } (chained)
 */
std::expected<std::unique_ptr<IfStmt>, Diagnostic> Parser::parse_if_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'if'

    auto condition = parse_expr();
    if (!condition) return std::unexpected(condition.error());

    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    // No else clause
    if (peek_token().get_type() != TokenType::tok_else) {
        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        nullptr);
    }

    // Else clause
    advance_token(); // eat 'else'

    // Else block: else { ... }
    if (peek_token().get_type() == TokenType::tok_open_brace) {
        auto else_body = parse_block();
        if (!else_body) return std::unexpected(else_body.error());

        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        std::move(else_body.value()));
    }
    // Else if: else if ...
    if (peek_token().get_type() == TokenType::tok_if) {
        std::vector<std::unique_ptr<Stmt>> elif_stmt;

        auto res = parse_if_stmt();
        if (!res) return std::unexpected(res.error());

        elif_stmt.emplace_back(std::move(res.value()));
        auto elif_body = std::make_unique<Block>(std::move(elif_stmt));
        return std::make_unique<IfStmt>(loc,
                                        std::move(condition.value()),
                                        std::move(body.value()),
                                        std::move(elif_body));
    }

    // Invalid else clause
    emit_error(error("invalid else clause")
                   .with_primary_label(span_from_token(peek_token()), "unexpected token here")
                   .with_help("else must be followed by a block `{` or another `if` statement")
                   .with_code("E0040")
                   .build());
    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid else clause"));
}

/**
 * Parses a while loop statement.
 *
 * @return std::expected<std::unique_ptr<WhileStmt>, Diagnostic> While loop AST or error.
 *
 * Format: while (condition) { body }
 */
std::expected<std::unique_ptr<WhileStmt>, Diagnostic> Parser::parse_while_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'while'

    auto condition = parse_expr();
    if (!condition) return std::unexpected(condition.error());

    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    return std::make_unique<WhileStmt>(loc, std::move(condition.value()), std::move(body.value()));
}

/**
 * Parses a for loop statement.
 *
 * @return std::expected<std::unique_ptr<ForStmt>, Diagnostic> For loop AST or error.
 *
 * Format: for variable in range { body }
 *
 * Creates implicit loop variable declaration (i64 type).
 * Validates loop variable and 'in' keyword syntax.
 */
std::expected<std::unique_ptr<ForStmt>, Diagnostic> Parser::parse_for_stmt() {
    SrcLocation loc = peek_token().get_start();
    advance_token(); // eat 'for'

    // Parse loop variable
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

    // Validate 'in' keyword
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

    // Parse range expression
    auto range = parse_expr();
    if (!range) return std::unexpected(range.error());

    // Parse loop body
    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    // Create loop variable declaration (implicit i64 type)
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

/**
 * Parses a variable declaration (let statement).
 *
 * @return std::expected<std::unique_ptr<LetStmt>, Diagnostic> Variable declaration AST or error.
 *
 * Format: let name: type = value;
 *
 * Validates:
 * - Colon after identifier
 * - Valid type annotation
 * - Assignment operator
 * - Initializer expression
 * - Semicolon terminator
 */
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

    // Validate assignment operator
    if (advance_token().get_type() != TokenType::tok_assign) {
        emit_error(error("missing assignment in variable declaration")
                       .with_primary_label(span_from_token(peek_token()), "expected `=` here")
                       .with_help("variables must be initialized with a value")
                       .with_note("variable syntax: `let name: type = value;`")
                       .with_code("E0023")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing assignment"));
    }

    // Parse initializer expression
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

    // Validate semicolon terminator
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
