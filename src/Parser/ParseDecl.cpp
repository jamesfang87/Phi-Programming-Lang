#include "Parser/Parser.hpp"

#include "AST/Decl.hpp"

std::expected<std::unique_ptr<FunDecl>, Diagnostic> Parser::parse_function_decl() {
    Token tok = advance_token();
    auto [_, line, col] = tok.get_start();

    // we expect the next token to be an identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        throw_parsing_error(line,
                            peek_token().get_start().col,
                            "Invalid function name",
                            "Expected identifier here");
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }
    std::string name = advance_token().get_lexeme();

    // here we handle the parameter list
    auto param_list = parse_list<ParamDecl>(TokenType::tok_open_paren,
                                            TokenType::tok_close_paren,
                                            &Parser::parse_param_decl);
    if (!param_list) return std::unexpected(param_list.error());

    // now we handle the return type. if there is a `->`, then we need to parse
    // otherwise, we set the return type to null
    std::optional<Type> return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == TokenType::tok_fun_return) {
        advance_token(); // eat `->`

        return_type = parse_type();
        if (!return_type) {
            successful = false; // set success flag to false
            return std::unexpected(Diagnostic());
        }
    }

    // now we parse the function body
    auto body = parse_block();
    if (!body) return std::unexpected(body.error());

    return std::make_unique<FunDecl>(SrcLocation{.path = path, .line = line, .col = col},
                                     std::move(name),
                                     return_type.value(),
                                     std::move(*param_list.value()),
                                     std::move(body.value()));
}

// <identifier>: type
std::expected<std::unique_ptr<ParamDecl>, Diagnostic> Parser::parse_param_decl() {
    // we expect the first token to be an identifier
    if (peek_token().get_type() != TokenType::tok_identifier) {
        // throw_parsing_error(
        //     peek_token().get_start().line,
        //     peek_token().get_start().col,
        //     "Parameter must be an identifier",
        //     std::format("Expected identifier here, instead found `{}`",
        //     peek_token().get_name()));
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }
    std::string name = advance_token().get_lexeme();

    // next we expect the current token to be `:`
    if (peek_token().get_type() != TokenType::tok_colon) {
        // throw_parsing_error(
        //     peek_token().get_start().line,
        //     peek_token().get_start().col,
        //     "`:` should follow identifier",
        //     std::format("Expected `:` here, instead found `{}`", peek_token().get_name()));
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }
    advance_token();

    // lastly, we expect a type
    auto type = parse_type();
    if (!type) {
        // throw_parsing_error(
        //     peek_token().get_start().line,
        //     peek_token().get_start().col,
        //     "Invalid type, type must be specified for parameter declarations",
        //     std::format("Expected valid type here, instead found `{}`",
        //     peek_token().get_name()));
        successful = false; // set success flag to false
        return std::unexpected(Diagnostic());
    }

    return std::make_unique<ParamDecl>(SrcLocation{.path = path,
                                                   .line = peek_token().get_start().line,
                                                   .col = peek_token().get_start().col},
                                       name,
                                       type.value());
}
