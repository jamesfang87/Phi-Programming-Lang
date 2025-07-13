#include "AST/Expr.hpp"
#include "Lexer/TokenType.hpp"
#include "Parser/Parser.hpp"
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <utility>

std::unique_ptr<Expr> Parser::parse_expr() { return pratt(0); }

std::optional<std::pair<int, int>> postfix_bp(TokenType op) {
    switch (op) {
        case TokenType::tok_open_paren: return {{110, -1}};   // Function call
        case TokenType::tok_open_bracket: return {{110, -1}}; // Array index
        default: return std::nullopt;
    }
}

std::optional<std::pair<int, int>> prefix_bp(TokenType op) {
    switch (op) {
        case TokenType::tok_sub:
        case TokenType::tok_bang: return {{-1, 90}};
        default: return std::nullopt;
    }
}

std::optional<std::pair<int, int>> infix_bp(TokenType op) {
    switch (op) {
        case TokenType::tok_add:
        case TokenType::tok_sub: return {{30, 31}};
        case TokenType::tok_mul:
        case TokenType::tok_div:
        case TokenType::tok_mod: return {{40, 41}};
        default: return std::nullopt;
    }
}

std::unique_ptr<Expr> Parser::pratt(int min_bp) {
    Token tok = advance_token();
    std::println("{}", tok.get_name());
    std::unique_ptr<Expr> lhs;
    switch (tok.get_type()) {
        // Grouping
        case TokenType::tok_open_paren: {
            lhs = pratt(0);
            advance_token(); // for the last ')'
            break;
        }

        // Prefix operators
        case TokenType::tok_sub:
        case TokenType::tok_bang: {
            auto [ignore, r_bp] = prefix_bp(tok.get_type()).value();
            std::unique_ptr<Expr> rhs = pratt(r_bp);
            lhs = std::make_unique<UnaryOp>(std::move(rhs), tok);
            break;
        }

        // Literals
        case TokenType::tok_int_literal:
            lhs = std::make_unique<IntLiteral>(tok.get_start(), std::stoll(tok.get_lexeme()));
            break;
        case TokenType::tok_float_literal:
            lhs = std::make_unique<FloatLiteral>(tok.get_start(), std::stod(tok.get_lexeme()));
            break;

        case TokenType::tok_identifier:
            lhs = std::make_unique<DeclRefExpr>(tok.get_start(), tok.get_lexeme());
            break;

        default: {
            std::println("bad token");
            break;
        }
    }

    while (true) {
        // now we expect an operator
        Token op = peek_token();
        if (op.get_type() == TokenType::tok_eof) {
            break;
        }

        // check if there are postfix operators: (), [i], .foo(),
        if (postfix_bp(op.get_type()).has_value()) {
            auto [l_bp, r_bp] = postfix_bp(op.get_type()).value();
            if (l_bp < min_bp) {
                break;
            }

            lhs = parse_postfix(std::move(lhs));
            continue;
        }

        // otherwise check if there is an infix operator
        if (infix_bp(op.get_type()).has_value()) {
            auto [l_bp, r_bp] = infix_bp(op.get_type()).value();
            if (l_bp < min_bp) {
                break;
            }

            advance_token(); // consume the op
            lhs = std::make_unique<BinaryOp>(std::move(lhs), pratt(r_bp), op);
            continue;
        }

        // nothing else, so break
        break;
    }

    return lhs;
}

std::unique_ptr<Expr> Parser::parse_postfix(std::unique_ptr<Expr> expr) {
    // parse function call
    if (peek_token().get_type() == TokenType::tok_open_paren) {
        return parse_fun_call(std::move(expr));
    }

    // class member access
    if (peek_token().get_type() == TokenType::tok_member) {
    }

    // i remember that there should be an advance token here but it works when it's gone
    return expr;
}

std::unique_ptr<FunctionCall> Parser::parse_fun_call(std::unique_ptr<Expr> callee) {
    auto args = parse_list<Expr>(TokenType::tok_open_paren,
                                 TokenType::tok_close_paren,
                                 &Parser::parse_expr);
    if (args == nullptr) {
        return nullptr;
    }
    return std::make_unique<FunctionCall>(SrcLocation{.path = path,
                                                      .line = peek_token().get_start().line,
                                                      .col = peek_token().get_start().col},
                                          std::move(callee),
                                          std::move(*args));
}
