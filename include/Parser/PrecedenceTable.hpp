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
inline std::optional<std::pair<int, int>> infixBP(const TokenKind &type) {
  switch (type) {
  // Assignment (right-associative, lowest precedence)
  case TokenKind::EqualsKind:
  case TokenKind::PlusEqualsKind:
  case TokenKind::SubEqualsKind:
  case TokenKind::MulEqualKind:
  case TokenKind::DivEqualsKind:
  case TokenKind::ModEqualsKind:
    return std::make_pair(2, 1);

  // Logical OR
  case TokenKind::DoublePipeKind:
    return std::make_pair(3, 4);

  // Logical AND
  case TokenKind::DoubleAmpKind:
    return std::make_pair(5, 6);

  // Equality
  case TokenKind::DoubleEqualsKind:
  case TokenKind::BangEqualsKind:
    return std::make_pair(7, 8);

  // Relational
  case TokenKind::OpenCaretKind:
  case TokenKind::LessEqualKind:
  case TokenKind::CloseCaretKind:
  case TokenKind::GreaterEqualKind:
    return std::make_pair(9, 10);

  // Range operators
  case TokenKind::InclRangeKind:
  case TokenKind::ExclRangeKind:
    return std::make_pair(11, 12);

  // Additive
  case TokenKind::PlusKind:
  case TokenKind::MinusKind:
    return std::make_pair(13, 14);

  // Multiplicative
  case TokenKind::StarKind:
  case TokenKind::SlashKind:
  case TokenKind::PercentKind:
    return std::make_pair(15, 16);
  case TokenKind::PeriodKind:
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
inline std::optional<int> prefixBP(const TokenKind &type) {
  switch (type) {
  case TokenKind::MinusKind:       // Unary minus
  case TokenKind::BangKind:        // Logical NOT
  case TokenKind::DoublePlusKind:  // Pre-increment
  case TokenKind::DoubleMinusKind: // Pre-decrement
  case TokenKind::AmpKind:
  case TokenKind::StarKind:
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
inline std::optional<std::pair<int, int>> postfixBP(const TokenKind &type) {
  switch (type) {
  case TokenKind::DoublePlusKind:  // Post-increment
  case TokenKind::DoubleMinusKind: // Post-decrement
  case TokenKind::OpenParenKind:   // Function call
  case TokenKind::OpenBraceKind:   // Struct initializer
    return std::make_pair(19, 0);

  default:
    return std::nullopt;
  }
}

} // namespace phi
