#include "AST/Decl.hpp"
#include "Parser/Parser.hpp"
#include <format>

std::unique_ptr<FunctionDecl> Parser::parse_function_decl() {
    Token tok = advance_token();
    auto [_, line, col] = tok.get_start();

    // we expect the next token to be an identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        throw_parsing_error(line,
                            peek_token().get_start().col,
                            "Invalid function name",
                            "Expected identifier here");
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // here we handle the parameter list
    auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                            TokenType::tok_close_paren,
                                            &Parser::parse_param_decl);
    if (param_list == nullptr) {
        return nullptr;
    }

    // now we handle the return type. if there is a `->`, then we need to parse
    // otherwise, we set the return type to null
    std::optional<Type> return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == TokenType::tok_fun_return) {
        advance_token(); // eat `->`

        return_type = parse_type();
        if (!return_type.has_value()) {
            return nullptr;
        }
    }

    // now we parse the function body
    std::unique_ptr<Block> body = parse_block();
    if (body == nullptr) {
        return nullptr;
    }

    return std::make_unique<FunctionDecl>(SrcLocation{.path = path, .line = line, .col = col},
                                          std::move(name),
                                          return_type.value(),
                                          std::move(param_list),
                                          std::move(body));
}

// <identifier>: type
std::unique_ptr<ParamDecl> Parser::parse_param_decl() {
    // we expect the first token to be an identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        throw_parsing_error(
            peek_token().get_start().line,
            peek_token().get_start().col,
            "Parameter must be an identifier",
            std::format("Expected identifier here, instead found `{}`", peek_token().get_name()));
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // next we expect the current token to be `:`
    if (peek_token().get_type() != TokenType::tok_colon) {
        throw_parsing_error(
            peek_token().get_start().line,
            peek_token().get_start().col,
            "`:` should follow identifier",
            std::format("Expected `:` here, instead found `{}`", peek_token().get_name()));
        return nullptr;
    }
    advance_token();

    // lastly, we expect a type
    auto type = parse_type();
    if (!type.has_value()) {
        throw_parsing_error(
            peek_token().get_start().line,
            peek_token().get_start().col,
            "Invalid type, type must be specified for parameter declarations",
            std::format("Expected valid type here, instead found `{}`", peek_token().get_name()));
        return nullptr;
    }

    return std::make_unique<ParamDecl>(SrcLocation{.path = path,
                                                   .line = peek_token().get_start().line,
                                                   .col = peek_token().get_start().col},
                                       name,
                                       type.value());
}
