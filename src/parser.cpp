#include "parser.hpp"
#include "ast-util.hpp"
#include "ast.hpp"
#include "token.hpp"
#include <algorithm>
#include <cassert>
#include <cstddef>
#include <cstdint>
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

Token Parser::this_token() {
    if (token_it >= tokens.end()) {
        return {-1, -1, tok_eof, ""};
    }
    return *token_it;
}

Token Parser::advance_token() {
    Token ret = this_token();
    token_it++;
    return ret;
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

    std::string id = this_token().get_lexeme();
    auto it = primitive_map.find(id);
    if (it == primitive_map.end() && this_token().get_type() != tok_identifier) {
        throw_parsing_error(this_token().get_line(),
                            this_token().get_col(),
                           std::format("Invalid token found: {}", this_token().get_lexeme()),
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

// TODO: allow for more literals (string, bool, etc.)
std::unique_ptr<Expr> Parser::parse_primary_expr() {
    // for int literals
    if (this_token().get_type() == tok_int_literal) {
        int64_t value = std::stoll(this_token().get_lexeme());
        advance_token();
        return std::make_unique<IntLiteral>(SrcLocation{.path = path,
                                                        .line = this_token().get_line(),
                                                        .col = this_token().get_col()},
                                            value);
    }

    // for float literals
    if (this_token().get_type() == tok_float_literal) {
        double value = std::stod(this_token().get_lexeme());
        advance_token();
        return std::make_unique<FloatLiteral>(SrcLocation{.path = path,
                                                          .line = this_token().get_line(),
                                                          .col = this_token().get_col()},
                                              value);
    }

    if (this_token().get_type() == tok_identifier) {
        std::string name = this_token().get_lexeme();
        advance_token();
        return std::make_unique<DeclRefExpr>(SrcLocation{.path = path,
                                                         .line = this_token().get_line(),
                                                         .col = this_token().get_col()},
                                             name);
    }

    throw_parsing_error(this_token().get_line(),
                        this_token().get_col(),
                        std::format("Unexpected token '{}'", this_token().get_lexeme()),
                        "expected int, float, or variable here");
    return nullptr;
}

std::unique_ptr<Expr> Parser::parse_postfix_expr() {
    std::unique_ptr<Expr> expr = parse_primary_expr();
    if (expr == nullptr) {
        // TODO: add error message
        return nullptr;
    }

    // parse function call
    if (this_token().get_type() == tok_open_paren) {
    }

    // class member access
    if (this_token().get_type() == tok_member) {
    }
}

std::unique_ptr<ReturnStmt> Parser::parse_return_stmt() {
    int line = this_token().get_line(), col = this_token().get_col();
    advance_token(); // eat 'return'

    // case that there is no expression after 'return' (functions which return null)
    if (this_token().get_type() == tok_semicolon) {
        return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                            nullptr);
    }

    // functions which have a non-void return type
    // TODO: change to parse any expr
    std::unique_ptr<Expr> expr = parse_primary_expr();
    if (expr == nullptr) {
        throw_parsing_error(this_token().get_line(),
                            this_token().get_col(),
                            "Invalid expression found",
                            "Expected an expression after `return`");
        return nullptr;
    }

    // check that the line ends with a semicolon
    if (this_token().get_type() != tok_semicolon) {
        throw_parsing_error(this_token().get_line(),
                            this_token().get_col(),
                            "Invalid token found",
                            "Expected `;` after expression");
        return nullptr;
    }
    advance_token();

    return std::make_unique<ReturnStmt>(SrcLocation{.path = path, .line = line, .col = col},
                                        std::move(expr));
}

std::unique_ptr<Stmt> Parser::parse_stmt() {
    advance_token();
    if (this_token().get_type() == tok_return) {
        return parse_return_stmt();
    }
}

std::unique_ptr<Block> Parser::parse_block() {
    advance_token(); // eat {
    std::vector<std::unique_ptr<Stmt>> stmts;
    while (this_token().get_type() != tok_close_brace) {
        auto type = this_token().get_type();
        switch (type) {
            case tok_eof:
                throw_parsing_error(lines.size(),
                                    lines.back().size(),
                                    "No closing '}' found",
                                    "Expected `}` to close function body, instead reached EOF");
                std::println("this happensd");
                return nullptr;
            case tok_class:
                throw_parsing_error(this_token().get_line(),
                                    this_token().get_col(),
                                    "Class declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case tok_fun_return:
                throw_parsing_error(this_token().get_line(),
                                    this_token().get_col(),
                                    "Function return not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            case tok_fun:
                throw_parsing_error(this_token().get_line(),
                                    this_token().get_col(),
                                    "Function declaration not allowed inside function body",
                                    "Expected this to be a valid token");
                return nullptr;
            default:
                stmts.push_back(parse_return_stmt());
        }
        // parse_stmt();
    }

    advance_token(); // eat `}`
    return std::make_unique<Block>(std::move(stmts));
}

std::unique_ptr<FunctionDecl> Parser::parse_function_decl() {
    Token tok = advance_token();
    int line = tok.get_line(), col = tok.get_col(); // save line and col

    // we expect the next token to be an identifier
    if (this_token().get_type() != tok_identifier) {
        throw_parsing_error(line,
                            this_token().get_col(),
                            "Invalid function name",
                            "Expected identifier here");
        return nullptr;
    }
    std::string name = advance_token().get_lexeme();

    // here we handle the parameter list
    int open_paren_col = this_token().get_col();
    if (advance_token().get_type() != tok_open_paren) {
        throw_parsing_error(line,
                            open_paren_col,
                            "Missing open parenthesis",
                            "Expected missing parenthesis here");
        return nullptr;
    }

    // TODO: handle param list

    int close_paren_col = this_token().get_col();
    if (advance_token().get_type() != tok_close_paren) {
        throw_parsing_error(line,
                            close_paren_col,
                            "Missing closing parenthesis",
                            "Expected missing parenthesis here");
        return nullptr;
    }

    // now we handle the return type. if there is a `->`, then we need to parse
    // otherwise, we set the return type to null
    std::optional<Type> return_type = Type(Type::Primitive::null);
    if (this_token().get_type() == tok_fun_return) {
        advance_token(); // eat `->`

        return_type = parse_type();
        if (!return_type.has_value()) {
            return nullptr;
        }
    }

    if (this_token().get_type() != tok_open_brace) {
        throw_parsing_error(line,
                            this_token().get_col(),
                            "Missing open brace",
                            "Expected missing brace here");
        return nullptr;
    }

    // now we parse the function body
    std::unique_ptr<Block> body = parse_block();
    if (body == nullptr) {
        return nullptr;
    }

    return std::make_unique<FunctionDecl>(SrcLocation{.path = path, .line = line, .col = col},
                                          std::move(name),
                                          return_type.value(),
                                          std::move(body));
}

std::pair<std::vector<std::unique_ptr<FunctionDecl>>, bool> Parser::parse() {
    while (this_token().get_type() != tok_eof) {
        switch (this_token().get_type()) {
            case tok_fun: functions.push_back(parse_function_decl()); break;

            case tok_eof: return {std::move(functions), successful};

            default: advance_token();
        }
    }

    return {std::move(functions), successful};
}
