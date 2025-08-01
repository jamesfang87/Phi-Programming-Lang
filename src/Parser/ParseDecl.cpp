#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <utility>

#include "AST/Decl.hpp"

namespace phi {

/**
 * Parses a function declaration from the token stream.
 *
 * @return std::expected<std::unique_ptr<FunDecl>, Diagnostic> Function AST node or diagnostic
 * error.
 *
 * Parsing sequence:
 * 1. 'fun' keyword
 * 2. Function name identifier
 * 3. Parameter list in parentheses
 * 4. Optional return type (-> type)
 * 5. Function body block
 *
 * Emits detailed errors for each parsing stage with context-specific suggestions.
 */
std::expected<std::unique_ptr<FunDecl>, Diagnostic> Parser::parse_function_decl() {
    Token tok = advance_token(); // Eat 'fun'
    SrcLocation loc = tok.get_start();

    // Validate function name
    if (peek_token().get_type() != TokenType::tok_identifier) {
        emit_error(
            error("invalid function name")
                .with_primary_label(span_from_token(peek_token()), "expected function name here")
                .with_secondary_label(span_from_token(tok), "after `fun` keyword")
                .with_help("function names must be valid identifiers")
                .with_note("identifiers must start with a letter or underscore")
                .with_code("E0006")
                .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid function name"));
    }
    std::string name = advance_token().get_lexeme();

    // Parse parameter list
    auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                            TokenType::tok_close_paren,
                                            &Parser::parse_param_decl);
    if (!param_list) return std::unexpected(param_list.error());

    // Handle optional return type
    auto return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == TokenType::tok_fun_return) {
        advance_token(); // eat '->'
        auto res = parse_type();
        if (!res) {
            auto error = res.error();
            emit_error(std::move(error));
            return std::unexpected(res.error());
        }
        return_type = res.value();
    }

    // Parse function body
    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    return std::make_unique<FunDecl>(loc,
                                     std::move(name),
                                     return_type,
                                     std::move(param_list.value()),
                                     std::move(body.value()));
}

/**
 * Parses a typed binding (name: type) used in declarations.
 *
 * @return std::expected<std::pair<std::string, Type>, Diagnostic> Name-type pair or error.
 *
 * Format: identifier ':' type
 * Used in variable declarations, function parameters, etc.
 */
std::expected<std::pair<std::string, Type>, Diagnostic> Parser::parse_typed_binding() {
    // Parse identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        emit_error(
            error("expected identifier")
                .with_primary_label(span_from_token(peek_token()), "expected identifier here")
                .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "expected identifier"));
    }
    std::string name = advance_token().get_lexeme();

    // Parse colon separator
    if (peek_token().get_type() != TokenType::tok_colon) {
        emit_error(error("expected colon")
                       .with_primary_label(span_from_token(peek_token()), "expected `:` here")
                       .with_suggestion(span_from_token(peek_token()), ":", "add colon before type")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "expected colon"));
    }
    advance_token();

    // Parse type
    auto type = parse_type();
    if (!type) {
        emit_error(error("expected type")
                       .with_primary_label(span_from_token(peek_token()), "expected type here")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "expected type"));
    }

    return std::make_pair(name, std::move(type.value()));
}

/**
 * Parses a function parameter declaration.
 *
 * @return std::expected<std::unique_ptr<ParamDecl>, Diagnostic> Parameter AST or error.
 *
 * Wrapper around parse_typed_binding() that creates a ParamDecl node.
 */
std::expected<std::unique_ptr<ParamDecl>, Diagnostic> Parser::parse_param_decl() {
    auto binding = parse_typed_binding();
    if (!binding) {
        return std::unexpected(binding.error());
    }

    return std::make_unique<ParamDecl>(SrcLocation{.path = path,
                                                   .line = peek_token().get_start().line,
                                                   .col = peek_token().get_start().col},
                                       binding.value().first,
                                       binding.value().second);
}

} // namespace phi
