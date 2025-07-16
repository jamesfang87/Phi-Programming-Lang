#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/Token.hpp"
#include <expected>
#include <format>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#pragma once

class Parser {
public:
    // Constructor
    Parser(std::string_view src, std::string_view path, std::vector<Token>& tokens)
        : path(path),
          tokens(tokens),
          token_it(tokens.begin()),
          functions() {
        auto it = src.begin();
        while (it < src.end()) {
            auto line_start = it;
            while (it < src.end() && *it != '\n') {
                it++;
            }

            lines.emplace_back(line_start, it++);
        }
    }

    // Main parsing interface
    std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> parse();

private:
    // Member variables
    std::vector<std::string> lines;
    std::string path;
    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;
    bool successful = true;
    std::vector<std::unique_ptr<FunDecl>> functions;

    // Token management
    [[nodiscard]] Token peek_token() const {
        if (token_it >= tokens.end()) {
            return {{"", -1, -1}, {"", -1, -1}, TokenType::tok_eof, ""};
        }
        return *token_it;
    }

    Token advance_token() {
        Token ret = peek_token();
        token_it++;
        return ret;
    }

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

    // Expression parsing
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_expr();
    std::expected<std::unique_ptr<Expr>, Diagnostic> pratt(int min_bp);
    std::expected<std::unique_ptr<Expr>, Diagnostic> parse_postfix(std::unique_ptr<Expr> expr);
    std::expected<std::unique_ptr<FunCallExpr>, Diagnostic>
    parse_fun_call(std::unique_ptr<Expr> callee);

    // Utility functions
    // T is the type to return, F is the member function
    // of Parser to use
    template <typename T, typename F>
    std::expected<std::unique_ptr<std::vector<std::unique_ptr<T>>>, Diagnostic>
    parse_list(TokenType opening, TokenType closing, F fun) {
        // ensure the list is properly opened, emitting error if needed
        if (peek_token().get_type() != opening) {
            // throw_parsing_error(peek_token().get_start().line,
            //                     peek_token().get_start().col,
            //                     std::format("Missing `{}` to open list",
            //                     type_to_string(opening)), std::format("Expected missing `{}`
            //                     here, instead found `{}`",
            //                                 type_to_string(opening),
            // peek_token().get_name()));
            //
            successful = false; // set success flag to false
            return std::unexpected(Diagnostic());
        }
        advance_token();

        // how handle the list content
        std::vector<std::unique_ptr<T>> content;
        while (peek_token().get_type() != closing) {
            // use the function to parse the element we're at
            auto res = (this->*fun)();
            if (!res) {
                return std::unexpected(res.error());
            }
            content.push_back(std::move(res.value()));

            //  now we can either exit by seeing the closing token
            if (peek_token().get_type() == closing) {
                break;
            }

            // or continue, where we now expect a comma
            if (peek_token().get_type() != TokenType::tok_comma) {
                throw_parsing_error(peek_token().get_start().line,
                                    peek_token().get_start().col,
                                    "No comma found while parsing list",
                                    std::format("Expected `,` as delimiter, instead found {}",
                                                peek_token().get_name()));
                return nullptr;
            }
            advance_token(); // consume the `,`
        }
        advance_token(); // consume the closing token

        return std::make_unique<std::vector<std::unique_ptr<T>>>(std::move(content));
    }

    std::optional<Type> parse_type();
    void throw_parsing_error(int line,
                             int col,
                             std::string_view error,
                             std::string_view expected_message);

    void synchronize();
};
