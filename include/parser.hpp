#include "ast.hpp"
#include "token.hpp"
#include <memory>
#include <string>
#include <vector>

class Parser {
public:
    Parser(std::vector<Token>& tokens)
        : tokens(tokens),
          token_it(tokens.begin()),
          functions() {}
    Token advance_token();
    Token this_token();

    std::vector<std::unique_ptr<FunctionDecl>> parse();

    std::unique_ptr<FunctionDecl> parse_function_decl();

    void throw_parse_error(int line, int col, std::string error, std::string expected_message);

private:
    std::vector<Token>& tokens;
    std::vector<Token>::iterator token_it;
    bool successful = true;
    std::vector<std::unique_ptr<FunctionDecl>> functions;
};
