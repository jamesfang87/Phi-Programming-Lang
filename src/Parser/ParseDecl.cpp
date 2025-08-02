#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Parser/Parser.hpp"

#include <expected>
#include <memory>
#include <utility>

#include "AST/Decl.hpp"

namespace phi {

/**
 * Parses a function declaration from the token stream.
 *
 * @return std::unique_ptr<FunDecl> Function AST node or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Parsing sequence:
 * 1. 'fun' keyword
 * 2. Function name identifier
 * 3. Parameter list in parentheses
 * 4. Optional return type (-> type)
 * 5. Function body block
 *
 * Handles comprehensive error recovery and validation at each step.
 */
std::unique_ptr<FunDecl> Parser::parse_function_decl() {
    Token tok = advance_token(); // Eat 'fun'
    SrcLocation loc = tok.get_start();

    // Validate function name
    if (peek_token().get_type() != TokenType::tok_identifier) {
        error("invalid function name")
            .with_primary_label(span_from_token(peek_token()), "expected function name here")
            .with_secondary_label(span_from_token(tok), "after `fun` keyword")
            .with_help("function names must be valid identifiers")
            .with_note("identifiers must start with a letter or underscore")
            .with_code("E0006")
            .emit(*diagnostic_manager);
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // Parse parameter list
    auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                            TokenType::tok_close_paren,
                                            &Parser::parse_param_decl);
    if (!param_list) return nullptr;

    // Handle optional return type
    auto return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == TokenType::tok_fun_return) {
        advance_token(); // eat '->'
        auto res = parse_type();
        if (!res) return nullptr;
        return_type = res.value();
    }

    // Parse function body
    auto body = parse_block();
    if (!body) return nullptr;

    return std::make_unique<FunDecl>(loc,
                                     std::move(name),
                                     return_type,
                                     std::move(param_list.value()),
                                     std::move(body));
}

/**
 * Parses a typed binding (name: type) used in declarations.
 *
 * @return std::optional<std::pair<std::string, Type>> Name-type pair or nullopt on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Format: identifier ':' type
 * Used in variable declarations, function parameters, etc.
 */
std::optional<std::pair<std::string, Type>> Parser::parse_typed_binding() {
    // Parse identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        error("expected identifier")
            .with_primary_label(span_from_token(peek_token()), "expected identifier here")
            .emit(*diagnostic_manager);
        return std::nullopt;
    }
    std::string name = advance_token().get_lexeme();

    // Parse colon separator
    if (peek_token().get_type() != TokenType::tok_colon) {
        error("expected colon")
            .with_primary_label(span_from_token(peek_token()), "expected `:` here")
            .with_suggestion(span_from_token(peek_token()), ":", "add colon before type")
            .emit(*diagnostic_manager);
        return std::nullopt;
    }
    advance_token();

    // Parse type
    auto type = parse_type();
    if (!type) {
        return std::nullopt;
    }

    return std::make_pair(name, std::move(type.value()));
}

/**
 * Parses a function parameter declaration.
 *
 * @return std::unique_ptr<ParamDecl> Parameter AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Wrapper around parse_typed_binding() that creates a ParamDecl node.
 */
std::unique_ptr<ParamDecl> Parser::parse_param_decl() {
    auto binding = parse_typed_binding();
    if (!binding) {
        return nullptr;
    }

    return std::make_unique<ParamDecl>(SrcLocation{.path = path,
                                                   .line = peek_token().get_start().line,
                                                   .col = peek_token().get_start().col},
                                       binding->first,
                                       binding->second);
}

} // namespace phi
