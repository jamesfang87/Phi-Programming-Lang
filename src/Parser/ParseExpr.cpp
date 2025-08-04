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
std::unique_ptr<Expr> Parser::parseExpr() {
  auto res = pratt(0);
  if (!res) {
    std::println("Error parsing expression");
    return nullptr;
  }
  return res;
}

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
std::optional<std::pair<int, int>> infixBP(const TokenType &type) {
  switch (type) {
  // Assignment (right-associative, lowest precedence)
  case TokenType::tokEquals:
    return std::make_pair(2, 1);

  // Logical OR
  case TokenType::tokDoublePipe:
    return std::make_pair(3, 4);

  // Logical AND
  case TokenType::tokDoubleAmp:
    return std::make_pair(5, 6);

  // Equality
  case TokenType::tokDoubleEquals:
  case TokenType::tokBangEquals:
    return std::make_pair(7, 8);

  // Relational
  case TokenType::tokLeftCaret:
  case TokenType::tokLessEqual:
  case TokenType::tokRightCaret:
  case TokenType::tokGreaterEqual:
    return std::make_pair(9, 10);

  // Range operators
  case TokenType::tokInclusiveRange:
  case TokenType::tokExclusiveRange:
    return std::make_pair(11, 12);

  // Additive
  case TokenType::tokPlus:
  case TokenType::tokMinus:
    return std::make_pair(13, 14);

  // Multiplicative
  case TokenType::tokStar:
  case TokenType::tokSlash:
  case TokenType::tokPercent:
    return std::make_pair(15, 16);

  default:
    return std::nullopt;
  }
}

/**
 * Determines binding power for prefix operators.
 *
 * @param type Token type to check
 * @return Optional pair of (ignored, right_bp) if operator is prefix
 */
std::optional<std::pair<int, int>> prefixBP(const TokenType &type) {
  switch (type) {
  case TokenType::tokMinus:       // Unary minus
  case TokenType::tokBang:        // Logical NOT
  case TokenType::tokDoublePlus:  // Pre-increment
  case TokenType::tokDoubleMinus: // Pre-decrement
    return std::make_pair(0, 17);
  default:
    return std::nullopt;
  }
}

/**
 * Determines binding power for postfix operators.
 *
 * @param type Token type to check
 * @return Optional pair of (left_bp, ignored) if operator is postfix
 */
std::optional<std::pair<int, int>> postfixBP(const TokenType &type) {
  switch (type) {
  case TokenType::tokDoublePlus:  // Post-increment
  case TokenType::tokDoubleMinus: // Post-decrement
  case TokenType::tokOpenParen:   // Function call
  case TokenType::tokPeriod:      // Member access
    return std::make_pair(19, 0);
  default:
    return std::nullopt;
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
std::unique_ptr<Expr> Parser::pratt(int minBP) {
  std::unique_ptr<Expr> lhs = parsePrefix(advanceToken());
  if (!lhs)
    return nullptr;

  // Process right-hand side and operators
  while (true) {
    Token op = peekToken();
    if (op.getTy() == TokenType::tokEOF)
      break;

    // Handle postfix operators
    if (postfixBP(op.getTy())) {
      auto [l, r] = postfixBP(op.getTy()).value();
      if (l < minBP)
        break;

      switch (op.getTy()) {
      case TokenType::tokDoublePlus:
      case TokenType::tokDoubleMinus:
        advanceToken();
        lhs = std::make_unique<UnaryOp>(std::move(lhs), op, false); // postfix
        break;
      default:
        auto res = parsePostfix(std::move(lhs));
        if (!res)
          return nullptr;
        lhs = std::move(res);
      }
      continue;
    }

    // Handle infix operators
    if (infixBP(op.getTy())) {
      auto [l, r] = infixBP(op.getTy()).value();
      if (l < minBP)
        break;

      advanceToken(); // consume operator

      // Special handling for range operators
      if (op.getTy() == TokenType::tokExclusiveRange ||
          op.getTy() == TokenType::tokInclusiveRange) {

        bool inclusive = op.getTy() == TokenType::tokInclusiveRange;
        auto end = pratt(r);
        if (!end)
          return nullptr;
        lhs = std::make_unique<RangeLiteral>(op.getStart(), std::move(lhs),
                                             std::move(end), inclusive);
      } else {
        // Regular binary operators
        auto res = pratt(r);
        if (!res)
          return nullptr;
        lhs = std::make_unique<BinaryOp>(std::move(lhs), std::move(res), op);
      }
      continue;
    }

    break; // No more operators to process
  }

  return lhs;
}

std::unique_ptr<Expr> Parser::parsePrefix(const Token &tok) {
  std::unique_ptr<Expr> lhs;
  switch (tok.getTy()) {
  // Grouping: ( expr )
  case TokenType::tokOpenParen: {
    auto res = pratt(0);
    if (!res)
      return nullptr;
    lhs = std::move(res);

    if (peekToken().getTy() != TokenType::tokRightParen) {
      error("missing closing parenthesis")
          .with_primary_label(spanFromToken(peekToken()), "expected `)` here")
          .with_help("parentheses must be properly matched")
          .emit(*diagnosticManager);
      return nullptr;
    }

    advanceToken(); // consume ')'
    break;
  }

  // Prefix operators: -a, !b, ++c
  case TokenType::tokMinus:       // -
  case TokenType::tokBang:        // !
  case TokenType::tokDoublePlus:  // ++
  case TokenType::tokDoubleMinus: // --
  {
    auto [ignore, r] = prefixBP(tok.getTy()).value();
    auto rhs = pratt(r);
    if (!rhs)
      return nullptr;
    lhs = std::make_unique<UnaryOp>(std::move(rhs), tok, true); // prefix
    break;
  }

  // Literals
  case TokenType::tokIntLiteral:
    lhs = std::make_unique<IntLiteral>(tok.getStart(),
                                       std::stoll(tok.getLexeme()));
    break;
  case TokenType::tokFloatLiteral:
    lhs = std::make_unique<FloatLiteral>(tok.getStart(),
                                         std::stod(tok.getLexeme()));
    break;
  case TokenType::tokStrLiteral:
    lhs = std::make_unique<StrLiteral>(tok.getStart(), tok.getLexeme());
    break;
  case TokenType::tokCharLiteral:
    lhs = std::make_unique<CharLiteral>(tok.getStart(), tok.getLexeme()[0]);
    break;
  case TokenType::tokTrue:
    lhs = std::make_unique<BoolLiteral>(tok.getStart(), true);
    break;
  case TokenType::tokFalse:
    lhs = std::make_unique<BoolLiteral>(tok.getStart(), false);
    break;

  // Identifiers
  case TokenType::tokIdentifier:
    lhs = std::make_unique<DeclRefExpr>(tok.getStart(), tok.getLexeme());
    break;

  default:
    return nullptr;
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
std::unique_ptr<Expr> Parser::parsePostfix(std::unique_ptr<Expr> expr) {
  // Function call: expr(...)
  if (peekToken().getTy() == TokenType::tokOpenParen) {
    return parseFunCall(std::move(expr));
  }

  // Member access: expr.ident (stubbed)
  if (peekToken().getTy() == TokenType::tokPeriod) {
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
std::unique_ptr<FunCallExpr>
Parser::parseFunCall(std::unique_ptr<Expr> callee) {
  auto args = parseList<Expr>(TokenType::tokOpenParen, TokenType::tokRightParen,
                              &Parser::parseExpr);

  // Check if there were errors during argument parsing
  if (!args)
    return nullptr;

  return std::make_unique<FunCallExpr>(callee->getLocation(), std::move(callee),
                                       std::move(args.value()));
}

} // namespace phi
