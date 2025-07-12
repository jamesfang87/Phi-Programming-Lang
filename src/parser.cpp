#include "parser.hpp"
#include "ast-util.hpp"
#include "ast.hpp"
#include "token.hpp"
#include <algorithm>
#include <cassert>
#include <cstddef>
#include <format>
#include <iostream>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <string_view>
#include <unordered_map>
#include <utility>
#include <vector>

std::pair<std::vector<std::unique_ptr<FunctionDecl>>, bool> Parser::parse() {
    while (peek_token().get_type() != tok_eof) {
        switch (peek_token().get_type()) {
            case tok_fun: functions.push_back(parse_function_decl()); break;

            case tok_eof: return {std::move(functions), successful};

            default: advance_token();
        }
    }

    return {std::move(functions), successful};
}

std::unique_ptr<FunctionDecl> Parser::parse_function_decl() {
    Token tok = advance_token();
    int line = tok.get_line(), col = tok.get_col(); // save line and col

    // we expect the next token to be an identifier
    if (peek_token().get_type() != tok_identifier) {
        throw_parsing_error(line,
                            peek_token().get_col(),
                            "Invalid function name",
                            "Expected identifier here");
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // here we handle the parameter list
    auto param_list =
        parse_list<ParamDecl>(tok_open_paren, tok_close_paren, &Parser::parse_param_decl);
    if (param_list == nullptr) {
        return nullptr;
    }

    // now we handle the return type. if there is a `->`, then we need to parse
    // otherwise, we set the return type to null
    std::optional<Type> return_type = Type(Type::Primitive::null);
    if (peek_token().get_type() == tok_fun_return) {
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
    if (peek_token().get_type() != tok_identifier) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "Parameter must be an identifier",
                            std::format("Expected identifier here, instead found `{}`",
                                        token_type_to_string(peek_token().get_type())));
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // next we expect the current token to be `:`
    if (peek_token().get_type() != tok_colon) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "`:` should follow identifier",
                            std::format("Expected `:` here, instead found `{}`",
                                        token_type_to_string(peek_token().get_type())));
        return nullptr;
    }
    advance_token();

    // lastly, we expect a type
    auto type = parse_type();
    if (!type.has_value()) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "Invalid type, type must be specified for parameter declarations",
                            std::format("Expected valid type here, instead found `{}`",
                                        token_type_to_string(peek_token().get_type())));
        return nullptr;
    }

    return std::make_unique<ParamDecl>(
        SrcLocation{.path = path, .line = peek_token().get_line(), .col = peek_token().get_col()},
        name,
        type.value());
}

std::unique_ptr<Block> Parser::parse_block() {
    // make sure that the block starts with an open brace, exit and emit error
    // otherwise
    if (peek_token().get_type() != tok_open_brace) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "Missing open brace",
                            "Expected missing brace here");
        return nullptr;
    }

    // otherwise, parse block. this part also handles the closing brace
    advance_token(); // eat {
    std::vector<std::unique_ptr<Stmt>> stmts;
    while (peek_token().get_type() != tok_close_brace) {
        auto type = peek_token().get_type();
        switch (type) {
            case tok_eof:
                throw_parsing_error(lines.size(),
                                    lines.back().size(),
                                    "No closing '}' found",
                                    "Expected `}` to close function body, instead reached EOF");
                return nullptr;
            case tok_class:
                throw_parsing_error(peek_token().get_line(),
                                    peek_token().get_col(),
                                    "Class declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case tok_fun_return:
                throw_parsing_error(peek_token().get_line(),
                                    peek_token().get_col(),
                                    "Function return not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case tok_fun:
                throw_parsing_error(peek_token().get_line(),
                                    peek_token().get_col(),
                                    "Function declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            default:
                stmts.push_back(parse_return_stmt());
                // parse_stmt();
        }
    }

    advance_token(); // eat `}`
    return std::make_unique<Block>(std::move(stmts));
}

std::unique_ptr<Expr> Parser::parse_postfix_expr() {
    std::unique_ptr<Expr> expr = parse_primary_expr();
    if (expr == nullptr) {
        // TODO: add error message
        return nullptr;
    }

    // parse function call
    if (peek_token().get_type() == tok_open_paren) {
        auto args = parse_list<Expr>(tok_open_paren, tok_close_paren, &Parser::parse_postfix_expr);
        if (args == nullptr) {
            return nullptr;
        }
        return std::make_unique<FunctionCall>(SrcLocation{.path = path,
                                                          .line = peek_token().get_line(),
                                                          .col = peek_token().get_col()},
                                              std::move(expr),
                                              std::move(*args));
    }

    // class member access
    if (peek_token().get_type() == tok_member) {
    }

    return expr;
}

std::unique_ptr<Expr> Parser::parse_primary_expr() {
    Token t = advance_token();
    switch (t.get_type()) {
        case tok_int_literal:
            return std::make_unique<IntLiteral>(
                SrcLocation{.path = path, .line = t.get_line(), .col = t.get_col()},
                std::stoll(t.get_lexeme()));
        case tok_float_literal:
            return std::make_unique<FloatLiteral>(
                SrcLocation{.path = path, .line = t.get_line(), .col = t.get_col()},
                std::stod(t.get_lexeme()));
        case tok_str_literal:
            return std::make_unique<StrLiteral>(
                SrcLocation{.path = path, .line = t.get_line(), .col = t.get_col()},
                t.get_lexeme());
        case tok_char_literal:
            return std::make_unique<CharLiteral>(
                SrcLocation{.path = path, .line = t.get_line(), .col = t.get_col()},
                t.get_lexeme().front());
        case tok_identifier:
            return std::make_unique<DeclRef>(
                SrcLocation{.path = path, .line = t.get_line(), .col = t.get_col()},
                t.get_lexeme());
        default:
            throw_parsing_error(t.get_line(),
                                t.get_col(),
                                std::format("Unexpected token '{}'", t.get_lexeme()),
                                "expected int, float, or variable here");
            return nullptr;
    }
}

std::unique_ptr<Stmt> Parser::parse_stmt() {
    advance_token();
    if (peek_token().get_type() == tok_return) {
        return parse_return_stmt();
    }

    // TODO: Handle other statement types
    return nullptr;
}

std::unique_ptr<ReturnStmt> Parser::parse_return_stmt() {
    int line = peek_token().get_line(), col = peek_token().get_col();
    advance_token(); // eat 'return'

    // case that there is no expression after 'return'
    // (functions which return null)
    if (peek_token().get_type() == tok_semicolon) {
        advance_token(); // eat ';'
        return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                            nullptr);
    }

    // functions which have a non-void return type
    // TODO: change to parse any expr
    std::unique_ptr<Expr> expr = parse_postfix_expr();
    if (expr == nullptr) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "Invalid expression found",
                            "Expected an expression after `return`");
        return nullptr;
    }

    // check that the line ends with a semicolon
    if (peek_token().get_type() != tok_semicolon) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            "Invalid token found",
                            "Expected `;` after expression");
        return nullptr;
    }
    advance_token();

    return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                        std::move(expr));
}
// T is the type to return, F is the member function of
// Parser to use
template <typename T, typename F>
std::unique_ptr<std::vector<std::unique_ptr<T>>>
Parser::parse_list(TokenType opening, TokenType closing, F fun) {
    // ensure the list is properly opened, emitting error if needed
    if (peek_token().get_type() != opening) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            std::format("Missing `{}` to open list", token_type_to_string(opening)),
                            std::format("Expected missing `{}` here, instead found `{}`",
                                        token_type_to_string(opening),
                                        token_type_to_string(peek_token().get_type())));
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
        if (peek_token().get_type() != tok_comma) {
            throw_parsing_error(peek_token().get_line(),
                                peek_token().get_col(),
                                "No comma found while parsing list",
                                std::format("Expected `,` as delimiter, instead found {}",
                                            token_type_to_string(peek_token().get_type())));
            return nullptr;
        }
        advance_token(); // consume the `,`
    }
    advance_token(); //

    return std::make_unique<std::vector<std::unique_ptr<T>>>(std::move(content));
}

std::optional<Type> Parser::parse_type() {
    const std::unordered_map<std::string, Type::Primitive> primitive_map = {
        {"i8", Type::Primitive::i8},
        {"i16", Type::Primitive::i16},
        {"i32", Type::Primitive::i32},
        {"i64", Type::Primitive::i64},
        {"u8", Type::Primitive::u8},
        {"u16", Type::Primitive::u16},
        {"u32", Type::Primitive::u32},
        {"u64", Type::Primitive::u64},
        {"f32", Type::Primitive::f32},
        {"f64", Type::Primitive::f64},
        {"str", Type::Primitive::str},
        {"char", Type::Primitive::character},
        {"null", Type::Primitive::null}};

    std::string id = peek_token().get_lexeme();
    auto it = primitive_map.find(id);
    if (it == primitive_map.end() && peek_token().get_type() != tok_identifier) {
        throw_parsing_error(peek_token().get_line(),
                            peek_token().get_col(),
                            std::format("Invalid token found: {}", peek_token().get_lexeme()),
                            "Expected a valid type here");
        return std::nullopt;
    }

    advance_token();
    if (it == primitive_map.end()) {
        return Type(id);
    } else {
        return Type(it->second);
    }
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
}
