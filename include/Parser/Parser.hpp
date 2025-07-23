#pragma once

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"
#include <expected>
#include <format>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

namespace phi {

class Parser {
public:
    // Constructor
    Parser(std::string_view src,
           std::string_view path,
           std::vector<Token>& tokens,
           std::shared_ptr<DiagnosticManager> diagnostic_manager)
        : path(path),
          tokens(tokens),
          token_it(tokens.begin()),
          diagnostic_manager_(std::move(diagnostic_manager)),
          successful(true) {

        // Split source into lines for diagnostic rendering
        split_source_into_lines(src);

        // Register this source file with the diagnostic manager's source manager
        if (diagnostic_manager_->source_manager()) {
            diagnostic_manager_->source_manager()->add_source_file(std::string(path), src);
        }
    }

    // Main parsing interface
    std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> parse();

    // Check if parsing was successful (no errors)
    bool is_successful() const { return successful; }

    // Get the current file path
    const std::string& file_path() const { return path; }

private:
    // Member variables
    std::string path;
    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;
    std::shared_ptr<DiagnosticManager> diagnostic_manager_;
    std::vector<std::string_view> lines;
    bool successful;
    std::vector<std::unique_ptr<FunDecl>> functions;

    // Token management
    [[nodiscard]] Token peek_token() const {
        if (token_it >= tokens.end()) {
            // Return a synthetic EOF token
            SrcLocation eof_loc{path,
                                static_cast<int>(lines.size()),
                                lines.empty() ? 0 : static_cast<int>(lines.back().size())};
            return Token{eof_loc, eof_loc, TokenType::tok_eof, ""};
        }
        return *token_it;
    }

    Token advance_token() {
        Token ret = peek_token();
        if (token_it < tokens.end()) {
            token_it++;
        }
        return ret;
    }

    // Check if we're at the end of input
    bool at_eof() const {
        return token_it >= tokens.end() || peek_token().get_type() == TokenType::tok_eof;
    }

    // Split source code into lines for diagnostic rendering
    void split_source_into_lines(std::string_view src) {
        auto it = src.begin();
        while (it < src.end()) {
            auto line_start = it;
            while (it < src.end() && *it != '\n') {
                it++;
            }
            lines.emplace_back(line_start, it);
            if (it < src.end()) {
                it++; // Skip the '\n'
            }
        }
    }

    // Enhanced error reporting methods

    // Emit an error and mark parsing as failed
    void emit_error(Diagnostic&& diagnostic) {
        successful = false;
        diagnostic_manager_->emit(diagnostic);
    }

    // Emit a warning (doesn't mark parsing as failed)
    void emit_warning(Diagnostic&& diagnostic) { diagnostic_manager_->emit(diagnostic); }

    // Create a source span from a token
    SourceSpan span_from_token(const Token& token) const {
        return SourceSpan(token.get_start(), token.get_end());
    }

    // Create a source span from two tokens (inclusive range)
    SourceSpan span_from_tokens(const Token& start_token, const Token& end_token) const {
        return SourceSpan(start_token.get_start(), end_token.get_end());
    }

    // Emit a "expected X found Y" error
    void emit_expected_found_error(const std::string& expected, const Token& found_token) {
        emit_error(error(std::format("expected {}, found `{}`", expected, found_token.get_lexeme()))
                       .with_primary_label(span_from_token(found_token),
                                           std::format("expected {} here", expected))
                       .build());
    }

    // Emit an "unexpected token" error with suggestions
    void emit_unexpected_token_error(const Token& token,
                                     const std::vector<std::string>& expected_tokens = {}) {
        auto builder = error(std::format("unexpected token `{}`", token.get_lexeme()))
                           .with_primary_label(span_from_token(token), "unexpected token");

        if (!expected_tokens.empty()) {
            std::string suggestion = "expected ";
            for (size_t i = 0; i < expected_tokens.size(); ++i) {
                if (i > 0) {
                    suggestion += (i == expected_tokens.size() - 1) ? " or " : ", ";
                }
                suggestion += "`" + expected_tokens[i] + "`";
            }
            builder.with_help(suggestion);
        }

        emit_error(std::move(builder).build());
    }

    // Emit an "unclosed delimiter" error with helpful context
    void emit_unclosed_delimiter_error(const Token& opening_token,
                                       const std::string& expected_closing) {
        emit_error(
            error("unclosed delimiter")
                .with_primary_label(span_from_token(opening_token),
                                    std::format("unclosed `{}`", opening_token.get_lexeme()))
                .with_help(std::format("expected `{}` to close this delimiter", expected_closing))
                .with_note("delimiters must be properly matched")
                .build());
    }

    // Enhanced error recovery - skip to synchronization points
    void synchronize() {
        advance_token(); // Skip the problematic token

        while (!at_eof()) {
            // Synchronize on statement boundaries
            switch (peek_token().get_type()) {
                case TokenType::tok_fun:
                case TokenType::tok_class:
                case TokenType::tok_return:
                case TokenType::tok_if:
                case TokenType::tok_while:
                case TokenType::tok_for:
                case TokenType::tok_semicolon: return;
                default: break;
            }
            advance_token();
        }
    }

    // Synchronize to a specific set of tokens
    // Returns true if one of the target tokens was found, false if EOF reached
    bool sync_to(std::initializer_list<TokenType> target_tokens) {
        while (!at_eof()) {
            for (TokenType target : target_tokens) {
                if (peek_token().get_type() == target) {
                    return true;
                }
            }
            advance_token();
        }
        return false; // Reached EOF without finding target
    }

    // Synchronize to a specific token type
    // Returns true if the target token was found, false if EOF reached
    bool sync_to(TokenType target_token) {
        while (!at_eof() && peek_token().get_type() != target_token) {
            advance_token();
        }
        return !at_eof();
    }

    // Parsing functions
    std::expected<std::unique_ptr<FunDecl>, Diagnostic> parse_function_decl();
    std::expected<std::unique_ptr<ParamDecl>, Diagnostic> parse_param_decl();
    std::expected<std::unique_ptr<VarDeclStmt>, Diagnostic> parse_var_decl();
    std::expected<std::unique_ptr<Block>, Diagnostic> parse_block();

    // Parsing for statements
    std::expected<std::unique_ptr<Stmt>, Diagnostic> parse_stmt();
    std::expected<std::unique_ptr<ReturnStmt>, Diagnostic> parse_return_stmt();
    std::expected<std::unique_ptr<IfStmt>, Diagnostic> parse_if_stmt();
    std::expected<std::unique_ptr<WhileStmt>, Diagnostic> parse_while_stmt();
    std::expected<std::unique_ptr<ForStmt>, Diagnostic> parse_for_stmt();

    // Expression parsing
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_expr();
    std::expected<std::unique_ptr<Expr>, Diagnostic> pratt(int min_bp);
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_postfix(std::unique_ptr<Expr> expr);
    std::expected<std::unique_ptr<FunCallExpr>, Diagnostic>
    parse_fun_call(std::unique_ptr<Expr> callee);

    // Enhanced list parsing with better error messages
    template <typename T, typename F>
    std::expected<std::unique_ptr<std::vector<std::unique_ptr<T>>>, Diagnostic>
    parse_list(TokenType opening, TokenType closing, F fun, const std::string& context = "list") {

        // Check for opening delimiter
        if (peek_token().get_type() != opening) {
            emit_expected_found_error(type_to_string(opening), peek_token());
            return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
        }

        Token opening_token = advance_token();
        std::vector<std::unique_ptr<T>> content;

        while (!at_eof() && peek_token().get_type() != closing) {
            // Parse the next element
            auto result = (this->*fun)();
            if (!result) {
                synchronize();
                continue; // Try to continue parsing the rest of the list
            }
            content.push_back(std::move(result.value()));

            // Check for delimiter or closing
            if (peek_token().get_type() == closing) {
                break; // We're done
            } else if (peek_token().get_type() == TokenType::tok_comma) {
                advance_token(); // Consume comma

                // Check for trailing comma before closing
                if (peek_token().get_type() == closing) {
                    emit_warning(
                        warning("trailing comma in " + context)
                            .with_primary_label(span_from_token(peek_token()), "trailing comma")
                            .with_help("trailing commas are allowed but not necessary")
                            .build());
                    break;
                }
            } else {
                // Missing comma
                emit_error(
                    error("missing comma in " + context)
                        .with_primary_label(span_from_token(peek_token()), "expected `,` here")
                        .with_help("separate " + context + " elements with commas")
                        .build());

                // Try to continue without consuming the token
                if (peek_token().get_type() != closing) {
                    continue;
                }
            }
        }

        // Check for closing delimiter
        if (at_eof() || peek_token().get_type() != closing) {
            emit_unclosed_delimiter_error(opening_token, type_to_string(closing));
            return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed delimiter"));
        }

        advance_token(); // Consume closing delimiter
        return std::make_unique<std::vector<std::unique_ptr<T>>>(std::move(content));
    }

    // Type parsing
    std::expected<Type, Diagnostic> parse_type();

    // Utility functions
    bool expect_token(TokenType expected_type, const std::string& context = "");
    bool match_token(TokenType type);

    // Skip tokens until we find a recovery point
    void skip_until_recovery_point();

    // Check if the current token could start a statement
    bool is_statement_start() const;

    // Check if the current token could start an expression
    bool is_expression_start() const;

    // Convert token type to string for error messages

    // Create a null/error placeholder for expressions when recovery is needed
    std::unique_ptr<Expr> create_error_expr() const;

    // Create a null/error placeholder for statements when recovery is needed
    std::unique_ptr<Stmt> create_error_stmt() const;
};

} // namespace phi
