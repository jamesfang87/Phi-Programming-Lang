#include "Parser/Parser.hpp"

#include "Lexer/TokenType.hpp"

namespace phi {

Parser::Parser(const std::string_view src,
               const std::string_view path,
               std::vector<Token>& tokens,
               std::shared_ptr<DiagnosticManager> diagnostic_manager)
    : path(path),
      tokens(tokens),
      token_it(tokens.begin()),
      diagnostic_manager(std::move(diagnostic_manager)) {

    if (this->diagnostic_manager->source_manager()) {
        this->diagnostic_manager->source_manager()->add_source_file(std::string(path), src);
    }
}

std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> Parser::parse() {
    while (!at_eof()) {
        switch (peek_token().get_type()) {
            case TokenType::tok_fun: {
                auto res = parse_function_decl();
                if (res.has_value()) {
                    functions.push_back(std::move(res.value()));
                } else {
                    sync_to();
                }
                break;
            }
            default:
                emit_unexpected_token_error(peek_token(), {"fun"});
                sync_to();
                break;
        }
    }

    return {std::move(functions), !diagnostic_manager->has_errors()};
}

} // namespace phi
