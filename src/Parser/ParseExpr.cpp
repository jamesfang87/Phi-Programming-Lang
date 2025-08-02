#include "Lexer/Token.hpp"
#include "Parser/Parser.hpp"

#include <memory>
#include <print>

#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Entry point for expression parsing.
 *
 * @return std::unique_ptr<Expr> Expression AST or nullptr on error.
 *         Errors are emitted to DiagnosticManager.
 *
 * Uses Pratt parsing with operator precedence handling.
 */
std::unique_ptr<Expr> Parser::parse_expr() { return pratt(0); }

// =============================================================================
// OPERATOR PRECEDENCE TABLES
// =============================================================================

/**
 * Determines binding power for infix operators.
 *
 * @param type Token type to check
 * @return Optional pair of (left_bp, right_bp) if operator is infix
 *
 * Higher numbers = higher precedence.
 * Left-associative: (left_bp, right_bp) where right_bp = left_bp + 1
 * Right-associative: (left_bp, right_bp) where right_bp = left_bp
 */
std::optional<std::pair<int, int>> infix_bp(const TokenType& type) {
    switch (type) {
        // Assignment (right-associative, lowest precedence)
        case TokenType::tok_assign: return std::make_pair(2, 1);

        // Logical OR
        case TokenType::tok_or: return std::make_pair(3, 4);

        // Logical AND
        case TokenType::tok_and: return std::make_pair(5, 6);

        // Equality
        case TokenType::tok_equal:
        case TokenType::tok_not_equal: return std::make_pair(7, 8);

        // Relational
        case TokenType::tok_less:
        case TokenType::tok_less_equal:
        case TokenType::tok_greater:
        case TokenType::tok_greater_equal: return std::make_pair(9, 10);

        // Range operators
        case TokenType::tok_inclusive_range:
        case TokenType::tok_exclusive_range: return std::make_pair(11, 12);

        // Additive
        case TokenType::tok_add:
        case TokenType::tok_sub: return std::make_pair(13, 14);

        // Multiplicative
        case TokenType::tok_mul:
        case TokenType::tok_div:
        case TokenType::tok_mod: return std::make_pair(15, 16);

        default: return std::nullopt;
    }
}

/**
 * Determines binding power for prefix operators.
 *
 * @param type Token type to check
 * @return Optional pair of (ignored, right_bp) if operator is prefix
 */
std::optional<std::pair<int, int>> prefix_bp(const TokenType& type) {
    switch (type) {
        case TokenType::tok_sub:       // Unary minus
        case TokenType::tok_bang:      // Logical NOT
        case TokenType::tok_increment: // Pre-increment
        case TokenType::tok_decrement: // Pre-decrement
            return std::make_pair(0, 17);
        default: return std::nullopt;
    }
}

/**
 * Determines binding power for postfix operators.
 *
 * @param type Token type to check
 * @return Optional pair of (left_bp, ignored) if operator is postfix
 */
std::optional<std::pair<int, int>> postfix_bp(const TokenType& type) {
    switch (type) {
        case TokenType::tok_increment:  // Post-increment
        case TokenType::tok_decrement:  // Post-decrement
        case TokenType::tok_open_paren: // Function call
        case TokenType::tok_member:     // Member access
            return std::make_pair(19, 0);
        default: return std::nullopt;
    }
}

/**
 * Pratt parser implementation for expressions.
 *
 * @param min_bp Minimum binding power for current expression
 * @return std::unique_ptr<Expr> Expression AST or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 *
 * Recursive descent parser that handles:
 * - Prefix operators
 * - Literal values
 * - Identifiers
 * - Parenthesized expressions
 * - Postfix operations (function calls, member access)
 */
std::unique_ptr<Expr> Parser::pratt(int min_bp) {
    std::unique_ptr<Expr> lhs = parse_prefix(advance_token());
    if (!lhs) return nullptr;

    // Process right-hand side and operators
    while (true) {
        Token op = peek_token();
        if (op.get_type() == TokenType::tok_eof) break;

        // Handle postfix operators
        if (postfix_bp(op.get_type())) {
            auto [l_bp, r_bp] = postfix_bp(op.get_type()).value();
            if (l_bp < min_bp) break;

            switch (op.get_type()) {
                case TokenType::tok_increment:
                case TokenType::tok_decrement:
                    advance_token();
                    lhs = std::make_unique<UnaryOp>(std::move(lhs), op, false); // postfix
                    break;
                default:
                    auto res = parse_postfix(std::move(lhs));
                    if (!res) return nullptr;
                    lhs = std::move(res);
            }
            continue;
        }

        // Handle infix operators
        if (infix_bp(op.get_type())) {
            auto [l_bp, r_bp] = infix_bp(op.get_type()).value();
            if (l_bp < min_bp) break;

            advance_token(); // consume operator

            // Special handling for range operators
            if (op.get_type() == TokenType::tok_exclusive_range ||
                op.get_type() == TokenType::tok_inclusive_range) {

                bool inclusive = op.get_type() == TokenType::tok_inclusive_range;
                auto end = pratt(r_bp);
                if (!end) return nullptr;
                lhs = std::make_unique<RangeLiteral>(op.get_start(),
                                                     std::move(lhs),
                                                     std::move(end),
                                                     inclusive);
            } else {
                // Regular binary operators
                auto res = pratt(r_bp);
                if (!res) return nullptr;
                lhs = std::make_unique<BinaryOp>(std::move(lhs), std::move(res), op);
            }
            continue;
        }

        break; // No more operators to process
    }

    return lhs;
}

std::unique_ptr<Expr> Parser::parse_prefix(const Token& tok) {
    std::unique_ptr<Expr> lhs;
    switch (tok.get_type()) {
        // Grouping: ( expr )
        case TokenType::tok_open_paren: {
            auto res = pratt(0);
            if (!res) return nullptr;
            lhs = std::move(res);

            if (peek_token().get_type() != TokenType::tok_close_paren) {
                emit_error(
                    error("missing closing parenthesis")
                        .with_primary_label(span_from_token(peek_token()), "expected `)` here")
                        .with_help("parentheses must be properly matched")
                        .build());
                return nullptr;
            }

            advance_token(); // consume ')'
            break;
        }

        // Prefix operators: -a, !b, ++c
        case TokenType::tok_sub:       // -
        case TokenType::tok_bang:      // !
        case TokenType::tok_increment: // ++
        case TokenType::tok_decrement: // --
        {
            auto [ignore, r_bp] = prefix_bp(tok.get_type()).value();
            auto rhs = pratt(r_bp);
            if (!rhs) return nullptr;
            lhs = std::make_unique<UnaryOp>(std::move(rhs), tok, true); // prefix
            break;
        }

        // Literals
        case TokenType::tok_int_literal:
            lhs = std::make_unique<IntLiteral>(tok.get_start(), std::stoll(tok.get_lexeme()));
            break;
        case TokenType::tok_float_literal:
            lhs = std::make_unique<FloatLiteral>(tok.get_start(), std::stod(tok.get_lexeme()));
            break;
        case TokenType::tok_str_literal:
            lhs = std::make_unique<StrLiteral>(tok.get_start(), tok.get_lexeme());
            break;
        case TokenType::tok_char_literal:
            lhs = std::make_unique<CharLiteral>(tok.get_start(), tok.get_lexeme()[0]);
            break;
        case TokenType::tok_true: lhs = std::make_unique<BoolLiteral>(tok.get_start(), true); break;
        case TokenType::tok_false:
            lhs = std::make_unique<BoolLiteral>(tok.get_start(), false);
            break;

        // Identifiers
        case TokenType::tok_identifier:
            lhs = std::make_unique<DeclRefExpr>(tok.get_start(), tok.get_lexeme());
            break;

        default: {
            return nullptr;
        }
    }
    return lhs;
}

/**
 * Handles postfix expressions after primary expression is parsed.
 *
 * @param expr The left-hand side expression
 * @return std::unique_ptr<Expr> Resulting expression or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 *
 * Currently handles:
 * - Function calls: expr(...)
 * - Member access: expr.member (stubbed)
 */
std::unique_ptr<Expr> Parser::parse_postfix(std::unique_ptr<Expr> expr) {
    // Function call: expr(...)
    if (peek_token().get_type() == TokenType::tok_open_paren) {
        return parse_fun_call(std::move(expr));
    }

    // Member access: expr.ident (stubbed)
    if (peek_token().get_type() == TokenType::tok_member) {
        // Implementation pending
    }

    return expr;
}

/**
 * Parses a function call expression.
 *
 * @param callee The function being called
 * @return std::unique_ptr<FunCallExpr> Call AST or nullptr on error
 *         Errors are emitted to DiagnosticManager.
 */
std::unique_ptr<FunCallExpr> Parser::parse_fun_call(std::unique_ptr<Expr> callee) {
    auto args = parse_list<Expr>(TokenType::tok_open_paren,
                                 TokenType::tok_close_paren,
                                 &Parser::parse_expr);

    // Check if there were errors during argument parsing
    if (args && diagnostic_manager->has_errors()) {
        return nullptr;
    }

    return std::make_unique<FunCallExpr>(callee->get_location(),
                                         std::move(callee),
                                         std::move(args.value()));
}

} // namespace phi
