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

    Token advance_token();
    Token this_token();

    std::pair<std::vector<std::unique_ptr<FunctionDecl>>, bool> parse();

    std::unique_ptr<FunctionDecl> parse_function_decl();

    std::optional<Type> parse_type();
    std::unique_ptr<Block> parse_block();

    std::unique_ptr<Expr> parse_primary_expr();

    std::unique_ptr<Stmt> parse_stmt();
    std::unique_ptr<ReturnStmt> parse_return_stmt();

    void
    throw_parsing_error(int line, int col, std::string_view error, std::string_view expected_message);

private:
    std::vector<std::string> lines;
    std::string_view path;

    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;

    bool successful = true;

    std::vector<std::unique_ptr<FunctionDecl>> functions;
};
