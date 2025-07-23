#include "AST/Stmt.hpp"
#include "Parser/Parser.hpp"

#include "AST/Decl.hpp"
#include "SrcLocation.hpp"
#include <memory>
#include <print>

namespace phi {

std::expected<std::unique_ptr<FunDecl>, Diagnostic> Parser::parse_function_decl() {
    Token tok = advance_token();
    auto [_, line, col] = tok.get_start();

    // we expect the next token to be an identifier
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

    // here we handle the parameter list
    auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                            TokenType::tok_close_paren,
                                            &Parser::parse_param_decl);
    if (!param_list) return std::unexpected(param_list.error());

    // now we handle the return type. if there is a `->`, then we need to parse
    // otherwise, we set the return type to null
    Type return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == TokenType::tok_fun_return) {
        advance_token(); // eat `->`

        auto res = parse_type();
        if (!res) {
            auto error = res.error();
            emit_error(std::move(error));
            return std::unexpected(res.error());
        }
        return_type = res.value();
    }

    // now we parse the function body
    auto body = parse_block();
    if (!body) {
        // Error already emitted during block parsing
        return std::unexpected(body.error());
    }

    return std::make_unique<FunDecl>(SrcLocation{.path = path, .line = line, .col = col},
                                     std::move(name),
                                     return_type,
                                     std::move(*param_list.value()),
                                     std::move(body.value()));
}

// <identifier>: type
std::expected<std::unique_ptr<ParamDecl>, Diagnostic> Parser::parse_param_decl() {
    // we expect the first token to be an identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        emit_error(
            error("parameter must have a name")
                .with_primary_label(span_from_token(peek_token()), "expected identifier here")
                .with_help("parameter names must be valid identifiers")
                .with_note("parameters have the form: `name: type`")
                .with_code("E0008")
                .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid parameter name"));
    }
    std::string name = advance_token().get_lexeme();

    // next we expect the current token to be `:`
    if (peek_token().get_type() != TokenType::tok_colon) {
        emit_error(error("missing colon in parameter declaration")
                       .with_primary_label(span_from_token(peek_token()), "expected `:` here")
                       .with_help("parameters must specify their type with a colon")
                       .with_suggestion(span_from_token(peek_token()), ":", "add colon before type")
                       .with_note("parameter syntax: `name: type`")
                       .with_code("E0009")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing colon"));
    }
    advance_token();

    // lastly, we expect a type
    auto type = parse_type();
    if (!type) {
        emit_error(error("parameter must have a valid type")
                       .with_primary_label(span_from_token(peek_token()), "expected type here")
                       .with_help("specify a valid type after the colon")
                       .with_note("valid types include: int, float, bool, string")
                       .with_code("E0010")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid parameter type"));
    }

    return std::make_unique<ParamDecl>(SrcLocation{.path = path,
                                                   .line = peek_token().get_start().line,
                                                   .col = peek_token().get_start().col},
                                       name,
                                       type.value());
}

std::expected<std::unique_ptr<VarDeclStmt>, Diagnostic> Parser::parse_var_decl() {
    SrcLocation loc = peek_token().get_start();
    if (advance_token().get_type() != TokenType::tok_let) {
        emit_unexpected_token_error(peek_token(), {"let"});
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
    }

    if (peek_token().get_type() != TokenType::tok_identifier) {
        emit_error(
            error("variable declaration must have a name")
                .with_primary_label(span_from_token(peek_token()), "expected identifier here")
                .with_help("variable names must be valid identifiers")
                .with_note("variable declarations have the form: `let name: type = value;`")
                .with_code("E0020")
                .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid variable name"));
    }
    std::string name = advance_token().get_lexeme();

    // next we expect the current token to be `:`
    if (peek_token().get_type() != TokenType::tok_colon) {
        emit_error(error("missing colon in variable declaration")
                       .with_primary_label(span_from_token(peek_token()), "expected `:` here")
                       .with_help("variables must specify their type with a colon")
                       .with_note("variable syntax: `let name: type = value;`")
                       .with_code("E0021")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "missing colon"));
    }
    advance_token();

    // lastly, we expect a type
    auto type = parse_type();
    if (!type) {
        emit_error(error("variable must have a valid type")
                       .with_primary_label(span_from_token(peek_token()), "expected type here")
                       .with_help("specify a valid type after the colon")
                       .with_note("valid types include: int, float, bool, string")
                       .with_code("E0022")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "invalid variable type"));
    }

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

    return std::make_unique<VarDeclStmt>(
        loc,
        std::make_unique<VarDecl>(loc, name, type.value(), true, std::move(expr.value())));
}
} // namespace phi
