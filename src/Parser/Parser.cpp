#include "Parser/Parser.hpp"

#include <iostream>
#include <print>

#include "Lexer/Token.hpp"

std::pair<std::vector<std::unique_ptr<FunDecl>>, bool> Parser::parse() {
    while (peek_token().get_type() != TokenType::tok_eof) {
        switch (peek_token().get_type()) {
            case TokenType::tok_fun: {
                auto res = parse_function_decl();
                if (res.has_value()) {
                    functions.push_back(std::move(res.value()));
                }
                break;
            }
            case TokenType::tok_eof: return {std::move(functions), successful};

            default: advance_token();
        }
    }

    return {std::move(functions), successful};
}

void Parser::throw_parsing_error(int line,
                                 int col,
                                 std::string_view message,
                                 std::string_view expected_message) {
    successful = false; // set success flag to false

    // we have reached eof
    if (col == -1) {
        col = lines.back().size();
    }

    // convert line number to string to get its width
    std::string line_num_str = std::to_string(line);
    int gutter_width = line_num_str.size() + 2;

    std::println(std::cerr, "\033[31;1;4merror:\033[0m {}", message);
    std::println(std::cerr, "--> {}:{}:{}", path, line, col);

    std::string_view line_content = lines[line - 1];
    std::println(std::cerr, "{}|", std::string(gutter_width, ' '));
    std::println(std::cerr, " {} | {}", line, line_content);
    std::println(std::cerr,
                 "{}|{}^ {}\n",
                 std::string(std::max(1, gutter_width), ' '),
                 std::string(std::max(1, col), ' '),
                 expected_message);

    synchronize();
}

void Parser::synchronize() {
    // Skip tokens until we find a synchronization point
    while (peek_token().get_type() != TokenType::tok_eof) {
        switch (peek_token().get_type()) {
            // Statement terminators
            case TokenType::tok_semicolon:
                advance_token(); // Consume the semicolon
                return;

            // Block/scope boundaries
            case TokenType::tok_close_brace:

                advance_token(); // Consume the semicolon
                return;          // Let caller handle closing brace

            // End of file
            case TokenType::tok_eof: return;

            // Skip all other tokens
            default: advance_token(); break;
        }
    }
}
