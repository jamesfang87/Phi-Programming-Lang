//===----------------------------------------------------------------------===//
// OPERATOR PRECEDENCE TABLES
//===----------------------------------------------------------------------===//
#pragma once

#include <optional>
#include <utility>

#include "Lexer/TokenKind.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Precedence Functions - Operator binding power determination
//===----------------------------------------------------------------------===//

/**
 * @brief Determines binding power for infix operators
 *
 * Used by the Pratt parser to handle operator precedence and associativity.
 * Higher numbers indicate higher precedence (tighter binding).
 *
 * @param Kind Token type to check
 * @return Optional pair of (left_bp, right_bp) if operator is infix
 *
 * Associativity rules:
 * - Left-associative: right_bp = left_bp + 1
 * - Right-associative: right_bp = left_bp
 */
inline std::optional<std::pair<int, int>> infixBP(const TokenKind &Kind) {
  switch (Kind) {
  //===--------------------------------------------------------------------===//
  // Assignment Operators (right-associative, lowest precedence)
  //===--------------------------------------------------------------------===//
  case TokenKind::Equals:
  case TokenKind::PlusEquals:
  case TokenKind::SubEquals:
  case TokenKind::MulEquals:
  case TokenKind::DivEquals:
  case TokenKind::ModEquals:
    return std::make_pair(2, 1);

  //===--------------------------------------------------------------------===//
  // Logical OR
  //===--------------------------------------------------------------------===//
  case TokenKind::DoublePipe:
    return std::make_pair(3, 4);

  //===--------------------------------------------------------------------===//
  // Logical AND
  //===--------------------------------------------------------------------===//
  case TokenKind::DoubleAmp:
    return std::make_pair(5, 6);

  //===--------------------------------------------------------------------===//
  // Equality Operators
  //===--------------------------------------------------------------------===//
  case TokenKind::DoubleEquals:
  case TokenKind::BangEquals:
    return std::make_pair(7, 8);

  //===--------------------------------------------------------------------===//
  // Relational Operators
  //===--------------------------------------------------------------------===//
  case TokenKind::OpenCaret:
  case TokenKind::LessEqual:
  case TokenKind::CloseCaret:
  case TokenKind::GreaterEqual:
    return std::make_pair(9, 10);

  //===--------------------------------------------------------------------===//
  // Range Operators
  //===--------------------------------------------------------------------===//
  case TokenKind::InclRange:
  case TokenKind::ExclRange:
    return std::make_pair(11, 12);

  //===--------------------------------------------------------------------===//
  // Additive Operators
  //===--------------------------------------------------------------------===//
  case TokenKind::Plus:
  case TokenKind::Minus:
    return std::make_pair(13, 14);

  //===--------------------------------------------------------------------===//
  // Multiplicative Operators
  //===--------------------------------------------------------------------===//
  case TokenKind::Star:
  case TokenKind::Slash:
  case TokenKind::Percent:
    return std::make_pair(15, 16);

  //===--------------------------------------------------------------------===//
  // Member Access (highest precedence)
  //===--------------------------------------------------------------------===//
  case TokenKind::Period:
    return std::make_pair(17, 18);

  default:
    return std::nullopt;
  }
}

/**
 * @brief Determines binding power for prefix operators
 *
 * Prefix operators have only right-binding power since they don't
 * have a left operand to consider.
 *
 * @param Kind Token type to check
 * @return Optional binding power if operator is prefix
 */
inline std::optional<int> prefixBP(const TokenKind &Kind) {
  switch (Kind) {
  //===--------------------------------------------------------------------===//
  // Unary Operators (high precedence)
  //===--------------------------------------------------------------------===//
  case TokenKind::Minus:       // Unary minus
  case TokenKind::Bang:        // Logical NOT
  case TokenKind::DoublePlus:  // Pre-increment
  case TokenKind::DoubleMinus: // Pre-decrement
  case TokenKind::Amp:         // Address-of
  case TokenKind::Star:        // Dereference
    return 17;

  default:
    return std::nullopt;
  }
}

/**
 * @brief Determines binding power for postfix operators
 *
 * Postfix operators have only left-binding power since they don't
 * have a right operand to consider.
 *
 * @param Kind Token type to check
 * @return Optional pair of (left_bp, ignored) if operator is postfix
 */
inline std::optional<std::pair<int, int>> postfixBP(const TokenKind &Kind) {
  switch (Kind) {
  //===--------------------------------------------------------------------===//
  // Postfix Operators (highest precedence)
  //===--------------------------------------------------------------------===//
  case TokenKind::DoublePlus:  // Post-increment
  case TokenKind::DoubleMinus: // Post-decrement
  case TokenKind::OpenParen:   // Function call
  case TokenKind::OpenBrace:   // Struct initializer
    return std::make_pair(19, 0);
  case TokenKind::Try:
    return std::make_pair(18, 0);

  default:
    return std::nullopt;
  }
}

} // namespace phi
