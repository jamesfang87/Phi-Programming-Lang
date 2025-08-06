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
std::optional<std::pair<int, int>> infixBP(const TokenKind &type) {
  switch (type) {
  // Assignment (right-associative, lowest precedence)
  case TokenKind::tokEquals:
    return std::make_pair(2, 1);

  // Logical OR
  case TokenKind::tokDoublePipe:
    return std::make_pair(3, 4);

  // Logical AND
  case TokenKind::tokDoubleAmp:
    return std::make_pair(5, 6);

  // Equality
  case TokenKind::tokDoubleEquals:
  case TokenKind::tokBangEquals:
    return std::make_pair(7, 8);

  // Relational
  case TokenKind::tokLeftCaret:
  case TokenKind::tokLessEqual:
  case TokenKind::tokRightCaret:
  case TokenKind::tokGreaterEqual:
    return std::make_pair(9, 10);

  // Range operators
  case TokenKind::tokInclusiveRange:
  case TokenKind::tokExclusiveRange:
    return std::make_pair(11, 12);

  // Additive
  case TokenKind::tokPlus:
  case TokenKind::tokMinus:
    return std::make_pair(13, 14);

  // Multiplicative
  case TokenKind::tokStar:
  case TokenKind::tokSlash:
  case TokenKind::tokPercent:
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
std::optional<std::pair<int, int>> prefixBP(const TokenKind &type) {
  switch (type) {
  case TokenKind::tokMinus:       // Unary minus
  case TokenKind::tokBang:        // Logical NOT
  case TokenKind::tokDoublePlus:  // Pre-increment
  case TokenKind::tokDoubleMinus: // Pre-decrement
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
std::optional<std::pair<int, int>> postfixBP(const TokenKind &type) {
  switch (type) {
  case TokenKind::tokDoublePlus:  // Post-increment
  case TokenKind::tokDoubleMinus: // Post-decrement
  case TokenKind::tokOpenParen:   // Function call
  case TokenKind::tokPeriod:      // Member access
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
    if (op.getType() == TokenKind::tokEOF)
      break;

    // Handle postfix operators
    if (postfixBP(op.getType())) {
      auto [l, r] = postfixBP(op.getType()).value();
      if (l < minBP)
        break;

      switch (op.getType()) {
      case TokenKind::tokDoublePlus:
      case TokenKind::tokDoubleMinus:
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
    if (infixBP(op.getType())) {
      auto [l, r] = infixBP(op.getType()).value();
      if (l < minBP)
        break;

      advanceToken(); // consume operator

      // Special handling for range operators
      if (op.getType() == TokenKind::tokExclusiveRange ||
          op.getType() == TokenKind::tokInclusiveRange) {

        bool inclusive = op.getType() == TokenKind::tokInclusiveRange;
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
  switch (tok.getType()) {
  // Grouping: ( expr )
  case TokenKind::tokOpenParen: {
    auto res = pratt(0);
    if (!res)
      return nullptr;
    lhs = std::move(res);

    if (peekToken().getType() != TokenKind::tokRightParen) {
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
  case TokenKind::tokMinus:       // -
  case TokenKind::tokBang:        // !
  case TokenKind::tokDoublePlus:  // ++
  case TokenKind::tokDoubleMinus: // --
  {
    auto [ignore, r] = prefixBP(tok.getType()).value();
    auto rhs = pratt(r);
    if (!rhs)
      return nullptr;
    lhs = std::make_unique<UnaryOp>(std::move(rhs), tok, true); // prefix
    break;
  }

  // Literals
  case TokenKind::tokIntLiteral:
    lhs = std::make_unique<IntLiteral>(tok.getStart(),
                                       std::stoll(tok.getLexeme()));
    break;
  case TokenKind::tokFloatLiteral:
    lhs = std::make_unique<FloatLiteral>(tok.getStart(),
                                         std::stod(tok.getLexeme()));
    break;
  case TokenKind::tokStrLiteral:
    lhs = std::make_unique<StrLiteral>(tok.getStart(), tok.getLexeme());
    break;
  case TokenKind::tokCharLiteral:
    lhs = std::make_unique<CharLiteral>(tok.getStart(), tok.getLexeme()[0]);
    break;
  case TokenKind::tokTrue:
    lhs = std::make_unique<BoolLiteral>(tok.getStart(), true);
    break;
  case TokenKind::tokFalse:
    lhs = std::make_unique<BoolLiteral>(tok.getStart(), false);
    break;

  // Identifiers
  case TokenKind::tokIdentifier:
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
  if (peekToken().getType() == TokenKind::tokOpenParen) {
    return parseFunCall(std::move(expr));
  }

  // Member access: expr.ident (stubbed)
  if (peekToken().getType() == TokenKind::tokPeriod) {
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
  auto args = parseList<Expr>(TokenKind::tokOpenParen, TokenKind::tokRightParen,
                              &Parser::parseExpr);

  // Check if there were errors during argument parsing
  if (!args)
    return nullptr;

  return std::make_unique<FunCallExpr>(callee->getLocation(), std::move(callee),
                                       std::move(args.value()));
}

} // namespace phi
