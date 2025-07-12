#include "Parser/Parser.hpp"
#include <format>

// T is the type to return, F is the member function of
// Parser to use
template <typename T, typename F>
std::unique_ptr<std::vector<std::unique_ptr<T>>>
Parser::parse_list(TokenType opening, TokenType closing, F fun) {
    // ensure the list is properly opened, emitting error if needed
    if (peek_token().get_type() != opening) {
        throw_parsing_error(peek_token().get_start().line,
                            peek_token().get_start().col,
                            std::format("Missing `{}` to open list", type_to_string(opening)),
                            std::format("Expected missing `{}` here, instead found `{}`",
                                        type_to_string(opening),
                                        peek_token().get_name()));
        return nullptr;
    }
    advance_token();

    // how handle the list content
    std::vector<std::unique_ptr<T>> content;
    while (peek_token().get_type() != closing) {
        // use the function to parse the element we're at
        auto res = (this->*fun)();
        if (res == nullptr) {
            return nullptr;
        }
        content.push_back(std::move(res));

        // now we can either exit by seeing the closing token
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
    advance_token(); //

    return std::make_unique<std::vector<std::unique_ptr<T>>>(std::move(content));
}
