#pragma once

#include <expected>
#include <memory>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

class Parser {
public:
    Parser(const std::string_view src,
           const std::string_view path,
           std::vector<Token>& tokens,
           std::shared_ptr<DiagnosticManager> diagnostic_manager);

    std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> parse();

private:
    // Member variables
    std::string path;
    std::vector<std::string_view> lines;
    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;
    std::vector<std::unique_ptr<FunDecl>> functions;
    std::shared_ptr<DiagnosticManager> diagnostic_manager;

    // Token manip
    [[nodiscard]] bool at_eof() const;
    [[nodiscard]] Token peek_token() const;
    Token advance_token();
    bool expect_token(TokenType expected_type, const std::string& context = "");
    bool match_token(TokenType type);

    void emit_error(Diagnostic&& diagnostic) { diagnostic_manager->emit(diagnostic); }
    void emit_warning(Diagnostic&& diagnostic) const { diagnostic_manager->emit(diagnostic); }
    void emit_expected_found_error(const std::string& expected, const Token& found_token);
    void emit_unexpected_token_error(const Token& token,
                                     const std::vector<std::string>& expected_tokens = {});
    void emit_unclosed_delimiter_error(const Token& opening_token,
                                       const std::string& expected_closing);

    // Enhanced error recovery - skip to synchronization points
    bool sync_to();
    bool sync_to(const std::initializer_list<TokenType> target_tokens);
    bool sync_to(const TokenType target_token);

    // Type parsing
    std::expected<Type, Diagnostic> parse_type();

    // Parsing functions
    std::expected<std::unique_ptr<FunDecl>, Diagnostic> parse_function_decl();
    std::expected<std::unique_ptr<ParamDecl>, Diagnostic> parse_param_decl();
    std::expected<std::unique_ptr<Block>, Diagnostic> parse_block();

    // Parsing for statements
    std::expected<std::unique_ptr<Stmt>, Diagnostic> parse_stmt();
    std::expected<std::unique_ptr<ReturnStmt>, Diagnostic> parse_return_stmt();
    std::expected<std::unique_ptr<IfStmt>, Diagnostic> parse_if_stmt();
    std::expected<std::unique_ptr<WhileStmt>, Diagnostic> parse_while_stmt();
    std::expected<std::unique_ptr<ForStmt>, Diagnostic> parse_for_stmt();
    std::expected<std::unique_ptr<LetStmt>, Diagnostic> parse_let_stmt();

    // Expression parsing
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_expr();
    std::expected<std::unique_ptr<Expr>, Diagnostic> pratt(int min_bp);
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_postfix(std::unique_ptr<Expr> expr);
    std::expected<std::unique_ptr<FunCallExpr>, Diagnostic>
    parse_fun_call(std::unique_ptr<Expr> callee);

    // Parsing utils
    std::expected<std::pair<std::string, Type>, Diagnostic> parse_typed_binding();
    template <typename T, typename F>
    std::expected<std::vector<std::unique_ptr<T>>, Diagnostic>
    parse_list(const TokenType opening,
               const TokenType closing,
               F fun,
               const std::string& context = "list") {
        // first we check for opening delimiter
        const Token opening_token = advance_token();
        if (opening_token.get_type() != opening) {
            emit_expected_found_error(type_to_string(opening), peek_token());
            return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
        }

        // now begin parsing the list
        std::vector<std::unique_ptr<T>> content;
        while (!at_eof() && peek_token().get_type() != closing) {
            auto result = (this->*fun)();
            if (!result) {
                sync_to({closing, TokenType::tok_comma});
                continue;
            }
            content.push_back(std::move(result.value()));

            // Check for delimiter or closing
            if (peek_token().get_type() == closing) {
                break;
            }

            if (peek_token().get_type() == TokenType::tok_comma) {
                advance_token();
            } else {
                emit_error(
                    error("missing comma in " + context)
                        .with_primary_label(span_from_token(peek_token()), "expected `,` here")
                        .with_help("separate " + context + " elements with commas")
                        .build());
            }
        }

        // Check for closing delimiter
        if (at_eof() || peek_token().get_type() != closing) {
            emit_unclosed_delimiter_error(opening_token, type_to_string(closing));
            return std::unexpected(Diagnostic(DiagnosticLevel::Error, "unclosed delimiter"));
        }

        advance_token(); // Consume closing delimiter
        return content;
    }
};
} // namespace phi
