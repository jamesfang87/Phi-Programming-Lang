#include "ast.hpp"
#include "token.hpp"
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

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
    std::pair<std::vector<std::unique_ptr<FunctionDecl>>, bool> parse();

private:
    // Member variables
    std::vector<std::string> lines;
    std::string path;
    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;
    bool successful = true;
    std::vector<std::unique_ptr<FunctionDecl>> functions;

    // Token management
    [[nodiscard]] Token peek_token() const {
        if (token_it >= tokens.end()) {
            return {-1, -1, tok_eof, ""};
        }
        return *token_it;
    }

    Token advance_token() {
        Token ret = peek_token();
        token_it++;
        return ret;
    }

    // Parsing functions
    std::unique_ptr<FunctionDecl> parse_function_decl();
    std::unique_ptr<ParamDecl> parse_param_decl();
    std::unique_ptr<Block> parse_block();

    // Parsing for statements
    std::unique_ptr<Stmt> parse_stmt();
    std::unique_ptr<ReturnStmt> parse_return_stmt();

    // Expression parsing
    std::unique_ptr<Expr> parse_expr();
    std::unique_ptr<Expr> parse_postfix_expr();
    std::unique_ptr<Expr> parse_primary_expr();

    // Utility functions
    // T is the type to return, F is the member function
    // of Parser to use
    template <typename T, typename F>
    std::unique_ptr<std::vector<std::unique_ptr<T>>>
    parse_list(TokenType opening, TokenType closing, F fun);
    std::optional<Type> parse_type();
    void throw_parsing_error(int line,
                             int col,
                             std::string_view error,
                             std::string_view expected_message);

    void synchronize();
};
