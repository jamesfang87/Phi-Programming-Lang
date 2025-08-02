#include "Parser/Parser.hpp"

#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Constructs a parser instance.
 *
 * @param src Source code string
 * @param path Source file path
 * @param tokens Token stream from lexer
 * @param diagnostic_manager Shared diagnostic manager
 */
Parser::Parser(const std::string_view src,
               const std::string_view path,
               std::vector<Token>& tokens,
               std::shared_ptr<DiagnosticManager> diagnostic_manager)
    : path(path),
      tokens(tokens),
      token_it(tokens.begin()),
      diagnostic_manager(std::move(diagnostic_manager)) {

    // Register source file with diagnostic manager
    if (this->diagnostic_manager->source_manager()) {
        this->diagnostic_manager->source_manager()->add_source_file(std::string(path), src);
    }
}

/**
 * Main parsing entry point.
 *
 * @return std::pair<std::vector<std::unique_ptr<FunDecl>>, bool>
 *         Pair of function declarations and success status
 *
 * Parses entire token stream, collecting function declarations.
 * Uses error recovery via sync_to() after errors.
 */
std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> Parser::parse() {
    while (!at_eof()) {
        switch (peek_token().get_type()) {
            case TokenType::tok_fun: {
                if (auto res = parse_function_decl()) {
                    functions.push_back(std::move(res));
                } else {
                    sync_to_top_lvl(); // Error recovery
                }
                break;
            }
            default:
                emit_unexpected_token_error(peek_token(), {"fun"});
                sync_to_top_lvl(); // Error recovery
        }
    }

    return {std::move(functions), !diagnostic_manager->has_errors()};
}

} // namespace phi
