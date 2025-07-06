#include "parser.hpp"
#include "ast.hpp"
#include "token.hpp"
#include <cstddef>
#include <iostream>
#include <memory>
#include <print>
#include <string>
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

void Parser::throw_parse_error(int line,
                               int col,
                               std::string message,
                               std::string expected_message) {
    successful = false; // set success flag to false

    // convert line number to string to get its width
    std::string line_num_str = std::to_string(line);
    size_t gutter_width = line_num_str.size() + 2;

    std::println(std::cerr, "{}", message);

    /*
    std::println(std::cerr, "--> {}:{}:{}", path, line, col);

    auto line_end = lexeme_line;
    while (line_end < src.end() && *line_end != '\n') {
        line_end++;
    }
    */
    std::string_view line_content; //(&*lexeme_line, static_cast<size_t>(line_end - lexeme_line));

    std::println(std::cerr, "{}|", std::string(gutter_width, ' '));
    std::println(std::cerr, " {} | {}", line, line_content);
    std::println(std::cerr,
                 "{}|{}^ {}\n",
                 std::string(gutter_width, ' '),
                 std::string(col, ' '),
                 expected_message);
}

std::unique_ptr<FunctionDecl> Parser::parse_function_decl() {
    Token tok = advance_token();
    int line = tok.get_line(), col = tok.get_col(); // save line and col

    // we expect the next token to be an identifier
    if (this_token().get_type() != tok_identifier) {
        throw_parse_error(line,
                          this_token().get_col(),
                          "Invalid function name",
                          "Expected identifier here");
    }
    std::string name = advance_token().get_lexeme();

    int open_paren_col = this_token().get_col();
    if (advance_token().get_type() != tok_open_paren) {
        throw_parse_error(line,
                          open_paren_col,
                          "Missing open parenthesis",
                          "Expected missing parenthesis here");
    }

    // TODO: handle param list

    int close_paren_col = this_token().get_col();
    if (advance_token().get_type() != tok_close_paren) {
        throw_parse_error(line,
                          close_paren_col,
                          "Missing closing parenthesis",
                          "Expected missing parenthesis here");
    }

    // now we handle the return type. if there is a =>, then we need to parse
    // otherwise, we set the return type to null
    Type return_type = Type(Type::Primitive::null);
    if (this_token().get_type() == tok_fun_return) {
        advance_token(); // discard the =>

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
            throw_parse_error(line,
                              this_token().get_col(),
                              "Invalid type for function return",
                              "Expected a valid type here");
        }

        if (it == primitive_map.end()) {
            return_type = Type(id);
        } else {
            return_type = Type(it->second);
        }
        advance_token();
    }

    if (this_token().get_type() != tok_open_brace) {
        throw_parse_error(line,
                          this_token().get_col(),
                          "Missing open brace",
                          "Expected missing brace here");
    }

    // now we parse the block
    Block fun_body;

    advance_token(); // consume {
    advance_token(); // consume }

    return std::make_unique<FunctionDecl>(line,
                                          col,
                                          std::move(name),
                                          return_type,
                                          std::make_unique<Block>(fun_body));
}

std::vector<std::unique_ptr<FunctionDecl>> Parser::parse() {
    while (this_token().get_type() != tok_eof) {
        switch (this_token().get_type()) {
            case tok_fun: functions.push_back(parse_function_decl()); break;

            case tok_eof: return std::move(functions);

            default: advance_token();
        }
    }

    return std::move(functions);
}
