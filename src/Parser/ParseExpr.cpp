#include "Parser/Parser.hpp"

#include "Diagnostics/Diagnostic.hpp"
#include "Lexer/TokenType.hpp"

namespace phi {

/**
 * Entry point for expression parsing.
 *
 * @return std::expected<std::unique_ptr<Expr>, Diagnostic> Expression AST or error.
 *
 * Uses Pratt parsing with operator precedence handling.
 */
std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::parse_expr() { return pratt(0); }

/**
 * Gets binding power for prefix operators.
 *
 * @param op Operator token type
 * @return Optional binding power (right-binding power)
 *
 * Higher values indicate tighter binding. Returns nullopt for non-prefix operators.
 */
std::optional<std::pair<int, int>> prefix_bp(const TokenType op) {
    switch (op) {
        case TokenType::tok_sub:       // -
        case TokenType::tok_bang:      // !
        case TokenType::tok_increment: // ++ prefix
        case TokenType::tok_decrement: // -- prefix
            return {{-1, 90}};         // Right-binding power
        default: return std::nullopt;
    }
}

/**
 * Gets binding power for infix operators.
 *
 * @param op Operator token type
 * @return Optional binding power (left, right) pair
 *
 * Returns nullopt for non-infix operators. Higher values = tighter binding.
 */
std::optional<std::pair<int, int>> infix_bp(const TokenType op) {
    switch (op) {
        case TokenType::tok_add: // +
        case TokenType::tok_sub: // -
            return {{30, 31}};
        case TokenType::tok_mul: // *
        case TokenType::tok_div: // /
        case TokenType::tok_mod: // %
            return {{40, 41}};
        // Range operators
        case TokenType::tok_exclusive_range: // ..
        case TokenType::tok_inclusive_range: // ..=
            return {{20, 21}};
        // Logical operators
        case TokenType::tok_and: // &&
            return {{18, 19}};
        case TokenType::tok_or: // ||
            return {{16, 17}};
        // Comparison operators
        case TokenType::tok_less:          // <
        case TokenType::tok_less_equal:    // <=
        case TokenType::tok_greater:       // >
        case TokenType::tok_greater_equal: // >=
            return {{25, 26}};
        // Equality operators
        case TokenType::tok_equal:     // ==
        case TokenType::tok_not_equal: // !=
            return {{22, 23}};
        default: return std::nullopt;
    }
}

/**
 * Gets binding power for postfix operators.
 *
 * @param op Operator token type
 * @return Optional binding power (left-binding power)
 *
 * Returns nullopt for non-postfix operators. Higher values = tighter binding.
 */
std::optional<std::pair<int, int>> postfix_bp(const TokenType op) {
    switch (op) {
        case TokenType::tok_open_paren:   // Function call ()
        case TokenType::tok_open_bracket: // Array index []
        case TokenType::tok_member:       // Member access .
        case TokenType::tok_increment:    // ++ postfix
        case TokenType::tok_decrement:    // -- postfix
            return {{110, -1}};           // Left-binding power
        default: return std::nullopt;
    }
}

/**
 * Pratt parser implementation for expressions.
 *
 * @param min_bp Minimum binding power for current expression
 * @return std::expected<std::unique_ptr<Expr>, Diagnostic> Expression AST or error
 *
 * Recursive descent parser that handles:
 * - Prefix operators
 * - Literal values
 * - Identifiers
 * - Parenthesized expressions
 * - Infix operators
 * - Postfix operators
 */
std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::pratt(int min_bp) {
    Token tok = advance_token();
    std::unique_ptr<Expr> lhs;

    // Handle left-hand side expressions
    switch (tok.get_type()) {
        // Grouping: ( expr )
        case TokenType::tok_open_paren: {
            auto res = pratt(0);
            if (!res) return std::unexpected(res.error());
            lhs = std::move(res.value());

            if (peek_token().get_type() != TokenType::tok_close_paren)
                return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));

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
            if (!rhs) return std::unexpected(rhs.error());
            lhs = std::make_unique<UnaryOp>(std::move(rhs.value()), tok, true); // prefix
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
        case TokenType::tok_str_literal:
            lhs = std::make_unique<StrLiteral>(tok.get_start(), tok.get_lexeme());
            break;
        case TokenType::tok_char_literal:
            lhs = std::make_unique<CharLiteral>(tok.get_start(), tok.get_lexeme()[0]);
            break;
        case TokenType::tok_false:
            lhs = std::make_unique<BoolLiteral>(tok.get_start(), false);
            break;

        // Identifiers
        case TokenType::tok_identifier:
            lhs = std::make_unique<DeclRefExpr>(tok.get_start(), tok.get_lexeme());
            break;

        default: {
            return std::unexpected(Diagnostic(DiagnosticLevel::Error, "parse error"));
        }
    }

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
                    if (!res) return std::unexpected(res.error());

                    lhs = std::move(res.value());
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
                if (!end) return std::unexpected(end.error());
                lhs = std::make_unique<RangeLiteral>(op.get_start(),
                                                     std::move(lhs),
                                                     std::move(end.value()),
                                                     inclusive);
            } else {
                // Regular binary operators
                auto res = pratt(r_bp);
                if (!res) return std::unexpected(res.error());

                lhs = std::make_unique<BinaryOp>(std::move(lhs), std::move(res.value()), op);
            }
            continue;
        }

        break; // No more operators to process
    }

    return lhs;
}

/**
 * Handles postfix expressions after primary expression is parsed.
 *
 * @param expr The left-hand side expression
 * @return std::expected<std::unique_ptr<Expr>, Diagnostic> Resulting expression or error
 *
 * Currently handles:
 * - Function calls: expr(...)
 * - Member access: expr.member (stubbed)
 */
std::expected<std::unique_ptr<Expr>, Diagnostic> Parser::parse_postfix(std::unique_ptr<Expr> expr) {
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
 * @return std::expected<std::unique_ptr<FunCallExpr>, Diagnostic> Call AST or error
 */
std::expected<std::unique_ptr<FunCallExpr>, Diagnostic>
Parser::parse_fun_call(std::unique_ptr<Expr> callee) {
    auto args = parse_list<Expr>(TokenType::tok_open_paren,
                                 TokenType::tok_close_paren,
                                 &Parser::parse_expr);
    if (!args) {
        return std::unexpected(args.error());
    }

    return std::make_unique<FunCallExpr>(callee->get_location(),
                                         std::move(callee),
                                         std::move(args.value()));
}

} // namespace phi
