#include "AST/Expr.hpp"
#include "Parser/Parser.hpp"
#include <format>
#include <memory>

std::unique_ptr<Expr> Parser::parse_expr() { return parse_postfix_expr(); }

std::unique_ptr<Expr> Parser::parse_postfix_expr() {
    std::unique_ptr<Expr> expr = parse_primary_expr();
    if (expr == nullptr) {
        // TODO: add error message
        return nullptr;
    }

    // parse function call
    if (peek_token().get_type() == TokenType::tok_open_paren) {
        auto args = parse_list<Expr>(TokenType::tok_open_paren,
                                     TokenType::tok_close_paren,
                                     &Parser::parse_postfix_expr);
        if (args == nullptr) {
            return nullptr;
        }
        return std::make_unique<FunctionCall>(SrcLocation{.path = path,
                                                          .line = peek_token().get_start().line,
                                                          .col = peek_token().get_start().col},
                                              std::move(expr),
                                              std::move(*args));
    }

    // class member access
    if (peek_token().get_type() == TokenType::tok_member) {
    }

    return expr;
}

std::unique_ptr<Expr> Parser::parse_primary_expr() {
    Token t = advance_token();
    switch (t.get_type()) {
        case TokenType::tok_int_literal:
            return std::make_unique<IntLiteral>(t.get_start(), std::stoll(t.get_lexeme()));
        case TokenType::tok_float_literal:
            return std::make_unique<FloatLiteral>(t.get_start(), std::stod(t.get_lexeme()));
        case TokenType::tok_str_literal:
            return std::make_unique<StrLiteral>(t.get_start(), t.get_lexeme());
        case TokenType::tok_char_literal:
            return std::make_unique<CharLiteral>(t.get_start(), t.get_lexeme().front());
        case TokenType::tok_identifier:
            return std::make_unique<DeclRefExpr>(t.get_start(), t.get_lexeme());
        default:
            throw_parsing_error(t.get_start().line,
                                t.get_start().col,
                                std::format("Unexpected token '{}'", t.get_lexeme()),
                                "expected int, float, or variable here");
            return nullptr;
    }
}
