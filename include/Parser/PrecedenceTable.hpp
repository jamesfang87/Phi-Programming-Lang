// =============================================================================
// OPERATOR PRECEDENCE TABLES
// =============================================================================
#pragma once

#include <optional>
#include <utility>

#include "Lexer/TokenKind.hpp"

namespace phi {

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
inline std::optional<std::pair<int, int>> infixBP(const TokenKind &Kind) {
  switch (Kind) {
  // Assignment (right-associative, lowest precedence)
  case TokenKind::Equals:
  case TokenKind::PlusEquals:
  case TokenKind::SubEquals:
  case TokenKind::MulEqual:
  case TokenKind::DivEquals:
  case TokenKind::ModEquals:
    return std::make_pair(2, 1);

  // Logical OR
  case TokenKind::DoublePipe:
    return std::make_pair(3, 4);

  // Logical AND
  case TokenKind::DoubleAmp:
    return std::make_pair(5, 6);

  // Equality
  case TokenKind::DoubleEquals:
  case TokenKind::BangEquals:
    return std::make_pair(7, 8);

  // Relational
  case TokenKind::OpenCaret:
  case TokenKind::LessEqual:
  case TokenKind::CloseCaret:
  case TokenKind::GreaterEqual:
    return std::make_pair(9, 10);

  // Range operators
  case TokenKind::InclRange:
  case TokenKind::ExclRange:
    return std::make_pair(11, 12);

  // Additive
  case TokenKind::Plus:
  case TokenKind::Minus:
    return std::make_pair(13, 14);

  // Multiplicative
  case TokenKind::Star:
  case TokenKind::Slash:
  case TokenKind::Percent:
    return std::make_pair(15, 16);
  case TokenKind::Period:
    return std::make_pair(17, 18);

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
inline std::optional<int> prefixBP(const TokenKind &Kind) {
  switch (Kind) {
  case TokenKind::Minus:       // Unary minus
  case TokenKind::Bang:        // Logical NOT
  case TokenKind::DoublePlus:  // Pre-increment
  case TokenKind::DoubleMinus: // Pre-decrement
  case TokenKind::Amp:
  case TokenKind::Star:
    return 17;
  default:
    return std::nullopt;
  }
}

/**
 * Determines binding power for postfix operators.
 *
 * @param type Token type to check
 * @param NoStructInit
 * @return Optional pair of (left_bp, ignored) if operator is postfix
 */
inline std::optional<std::pair<int, int>> postfixBP(const TokenKind &Kind) {
  switch (Kind) {
  case TokenKind::DoublePlus:  // Post-increment
  case TokenKind::DoubleMinus: // Post-decrement
  case TokenKind::OpenParen:   // Function call
  case TokenKind::OpenBrace:   // Struct initializer
    return std::make_pair(19, 0);

  default:
    return std::nullopt;
  }
}

} // namespace phi
