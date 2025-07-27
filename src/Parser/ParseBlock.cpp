#include "Parser/Parser.hpp"

namespace phi {

/**
 * Parses a block of statements enclosed in braces.
 *
 * @return std::expected<std::unique_ptr<Block>, Diagnostic> Block AST or error
 *
 * Handles:
 * - { statement1; statement2; ... }
 *
 * Performs extensive error recovery and validation:
 * - Validates opening/closing braces
 * - Recovers from nested declaration errors
 * - Synchronizes to statement boundaries after errors
 */
std::expected<std::unique_ptr<Block>, Diagnostic> Parser::parse_block() {
    // Validate opening brace
    if (peek_token().get_type() != TokenType::tok_open_brace) {
        emit_error(error("missing opening brace for block")
                       .with_primary_label(span_from_token(peek_token()), "expected `{` here")
                       .with_help("blocks must be enclosed in braces")
                       .with_code("E0001")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
    }

    const Token opening_brace = advance_token(); // eat '{'
    std::vector<std::unique_ptr<Stmt>> stmts;

    // Parse statements until closing brace
    while (peek_token().get_type() != TokenType::tok_close_brace) {
        switch (peek_token().get_type()) {
            case TokenType::tok_eof: // Unclosed block
                emit_error(
                    error("unclosed block")
                        .with_primary_label(span_from_token(opening_brace), "block opened here")
                        .with_secondary_label(span_from_token(peek_token()),
                                              "reached end of file here")
                        .with_help("add a closing brace `}` to close this block")
                        .with_note("blocks must have matching opening and closing braces")
                        .with_code("E0002")
                        .build());
                return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed block"));

            // Handle invalid declarations inside blocks
            case TokenType::tok_class: // Class in block
                emit_error(
                    error("class declarations are not allowed inside function bodies")
                        .with_primary_label(span_from_token(peek_token()), "class declaration here")
                        .with_secondary_label(span_from_token(opening_brace), "inside this block")
                        .with_help("move class declarations to the top level")
                        .with_note("classes must be declared at file scope, not inside functions")
                        .with_suggestion(span_from_token(peek_token()),
                                         "",
                                         "consider moving this to the top level")
                        .with_code("E0003")
                        .build());
                if (!sync_to({TokenType::tok_close_brace,
                              TokenType::tok_return,
                              TokenType::tok_if,
                              TokenType::tok_while,
                              TokenType::tok_for,
                              TokenType::tok_let})) {
                    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed block"));
                }
                continue;
            case TokenType::tok_fun_return: // Return type in block
                emit_error(
                    error("function return type syntax not allowed inside function body")
                        .with_primary_label(span_from_token(peek_token()), "unexpected `->` here")
                        .with_secondary_label(span_from_token(opening_brace), "inside this block")
                        .with_help("return type syntax is only valid in function declarations")
                        .with_note("did you mean to use a `return` statement instead?")
                        .with_code("E0013")
                        .build());
                if (!sync_to({TokenType::tok_close_brace,
                              TokenType::tok_return,
                              TokenType::tok_if,
                              TokenType::tok_while,
                              TokenType::tok_for,
                              TokenType::tok_let})) {
                    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed block"));
                }
                continue;
            case TokenType::tok_fun: // Nested function
                emit_error(
                    error("nested functions are not allowed")
                        .with_primary_label(span_from_token(peek_token()), "nested function here")
                        .with_secondary_label(span_from_token(opening_brace), "inside this block")
                        .with_help("move function declarations to the top level")
                        .with_note("functions cannot be declared inside other functions")
                        .with_code("E0004")
                        .build());
                if (!sync_to({TokenType::tok_close_brace,
                              TokenType::tok_return,
                              TokenType::tok_if,
                              TokenType::tok_while,
                              TokenType::tok_for,
                              TokenType::tok_let})) {
                    return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed block"));
                }
                continue;

            // Parse valid statements
            default:
                auto res = parse_stmt();
                if (!res) {
                    // Sync to next statement boundary
                    if (!sync_to({TokenType::tok_semicolon,
                                  TokenType::tok_close_brace,
                                  TokenType::tok_return,
                                  TokenType::tok_if,
                                  TokenType::tok_while,
                                  TokenType::tok_for,
                                  TokenType::tok_let})) {
                        return std::unexpected(
                            Diagnostic(DiagnosticLevel::Error, "unclosed block"));
                    }

                    // Consume semicolon if found
                    if (peek_token().get_type() == TokenType::tok_semicolon) {
                        advance_token();
                    }
                    continue;
                }
                stmts.push_back(std::move(res.value()));
        }
    }

    // Validate closing brace
    if (peek_token().get_type() != TokenType::tok_close_brace) {
        emit_error(error("missing closing brace for block")
                       .with_primary_label(span_from_token(peek_token()), "expected `}` here")
                       .with_help("blocks must be enclosed in braces")
                       .with_code("E0001")
                       .build());
        return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
    }
    advance_token(); // eat `}`

    return std::make_unique<Block>(std::move(stmts));
}

} // namespace phi
