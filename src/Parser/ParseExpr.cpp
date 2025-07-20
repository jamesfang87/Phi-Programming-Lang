#include "Diagnostics/Diagnostic.hpp"
#include "Parser/Parser.hpp"
#include <print>

std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::parse_expr() { return pratt(0); }

// Add to binding power functions
std::optional<std::pair<int, int>> prefix_bp(TokenType op) {
    switch (op) {
        case TokenType::tok_sub:
        case TokenType::tok_bang:
        case TokenType::tok_increment: // ++ prefix
        case TokenType::tok_decrement: // -- prefix
            return {{-1, 90}};
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
        // Range operators
        case TokenType::tok_exclusive_range:
        case TokenType::tok_inclusive_range: return {{20, 21}};
        // Comparison operators
        case TokenType::tok_less:
        case TokenType::tok_less_equal:
        case TokenType::tok_greater:
        case TokenType::tok_greater_equal: return {{25, 26}};
        // Equality operators
        case TokenType::tok_equal:
        case TokenType::tok_not_equal: return {{22, 23}};
        default: return std::nullopt;
    }
}

std::optional<std::pair<int, int>> postfix_bp(TokenType op) {
    switch (op) {
        case TokenType::tok_open_paren: return {{110, -1}};
        case TokenType::tok_open_bracket: return {{110, -1}};
        case TokenType::tok_member: return {{110, -1}};
        case TokenType::tok_increment: // ++ postfix
        case TokenType::tok_decrement: // -- postfix
            return {{110, -1}};
        default: return std::nullopt;
    }
}

std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::pratt(int min_bp) {
    Token tok = advance_token();
    std::unique_ptr<Expr> lhs;

    switch (tok.get_type()) {
        // Grouping
        case TokenType::tok_open_paren: {
            auto res = pratt(0);
            if (!res) return std::unexpected(res.error());
            lhs = std::move(res.value());

            if (peek_token().get_type() != TokenType::tok_close_paren) {
                // Error handling for missing ')'
                successful = false; // set success flag to false
                return std::unexpected(Diagnostic());
            }
            advance_token(); // consume ')'
            break;
        }

        // Prefix operators
        case TokenType::tok_sub:
        case TokenType::tok_bang:
        case TokenType::tok_increment:
        case TokenType::tok_decrement: {
            auto [ignore, r_bp] = prefix_bp(tok.get_type()).value();
            auto rhs = pratt(r_bp);
            if (!rhs) return std::unexpected(rhs.error());
            lhs = std::make_unique<UnaryOp>(std::move(rhs.value()), tok, true); // true = prefix
            break;
        }

        // Literals
        case TokenType::tok_int_literal:
            lhs = std::make_unique<IntLiteral>(tok.get_start(), std::stoll(tok.get_lexeme()));
            break;
        case TokenType::tok_float_literal:
            lhs = std::make_unique<FloatLiteral>(tok.get_start(), std::stod(tok.get_lexeme()));
            break;
        case TokenType::tok_true: lhs = std::make_unique<BoolLiteral>(tok.get_start(), true); break;
        case TokenType::tok_false:
            lhs = std::make_unique<BoolLiteral>(tok.get_start(), false);
            break;

        case TokenType::tok_identifier:
            lhs = std::make_unique<DeclRefExpr>(tok.get_start(), tok.get_lexeme());
            break;

        default: {
            // Error handling
            successful = false; // set success flag to false
            std::println("unexpected token");
            return std::unexpected(Diagnostic());
        }
    }

    while (true) {
        Token op = peek_token();
        if (op.get_type() == TokenType::tok_eof) break;

        // Handle postfix operators
        if (postfix_bp(op.get_type())) {
            auto [l_bp, r_bp] = postfix_bp(op.get_type()).value();
            if (l_bp < min_bp) break;

            // Handle different postfix types
            switch (op.get_type()) {
                // move this into parse postfix
                case TokenType::tok_increment:
                case TokenType::tok_decrement:
                    advance_token();
                    lhs = std::make_unique<UnaryOp>(std::move(lhs), op, false); // false = postfix
                    break;

                    // case TokenType::tok_member: lhs = parse_member_access(std::move(lhs)); break;

                default:
                    auto res = parse_postfix(std::move(lhs));
                    if (!res) {
                        std::println("error parsing postfix");
                        successful = false; // set success flag to false
                        return std::unexpected(res.error());
                    }
                    lhs = std::move(res.value());
            }
            continue;
        }

        // Handle infix operators
        if (infix_bp(op.get_type())) {
            auto [l_bp, r_bp] = infix_bp(op.get_type()).value();
            if (l_bp < min_bp) break;

            advance_token(); // consume the op

            // Special handling for range operators
            if (op.get_type() == TokenType::tok_exclusive_range ||
                op.get_type() == TokenType::tok_inclusive_range) {
                std::println("this happens");

                bool inclusive = (op.get_type() == TokenType::tok_inclusive_range);
                auto end = pratt(r_bp);
                if (!end) return std::unexpected(end.error());
                lhs = std::make_unique<RangeLiteral>(op.get_start(),
                                                     std::move(lhs),
                                                     std::move(end.value()),
                                                     inclusive);
            } else {
                // Regular binary operators
                auto res = pratt(r_bp);
                if (!res) {
                    std::println("error parsing regular binary expression");
                    successful = false;
                    return std::unexpected(res.error());
                }
                lhs = std::make_unique<BinaryOp>(std::move(lhs), std::move(res.value()), op);
            }
            continue;
        }

        break; // No more operators to process
    }

    return lhs;
}

std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::parse_postfix(std::unique_ptr<Expr> expr) {
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

std::expected<std::unique_ptr<FunCallExpr>, Diagnostic>
Parser::parse_fun_call(std::unique_ptr<Expr> callee) {
    auto args = parse_list<Expr>(TokenType::tok_open_paren,
                                 TokenType::tok_close_paren,
                                 &Parser::parse_expr);
    if (!args) {
        std::println("error parsing function call");
        successful = false;
        return std::unexpected(args.error());
    }

    return std::make_unique<FunCallExpr>(SrcLocation{.path = path,
                                                     .line = peek_token().get_start().line,
                                                     .col = peek_token().get_start().col},
                                         std::move(callee),
                                         std::move(*args.value()));
}
/*
// New helper function for member access
std::unique_ptr<Expr> Parser::parse_member_access(std::unique_ptr<Expr> expr) {
    advance_token(); // Consume '.' operator
    if (peek_token().get_type() != TokenType::tok_identifier) {
        // Error: expected identifier after '.'
        return expr;
    }

    Token member = advance_token();
    return std::make_unique<MemberExpr>(std::move(expr), member.get_lexeme(), member.get_start());
}
*/
