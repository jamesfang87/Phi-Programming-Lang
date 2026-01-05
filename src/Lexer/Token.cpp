/**
 * @file Token.cpp
 * @brief Implementation of the Token class and utility functions
 *
 * This file contains the implementation of the Token class constructor,
 * the token_type_to_string utility function for converting TokenType values
 * to human-readable strings, and the Token::to_string() method for debugging
 * output.
 */

#include "Lexer/Token.hpp"

#include <format>
#include <string>

namespace phi {

/**
 * @brief Formats the token as a human-readable string for debugging
 *
 * Creates a formatted string representation of the token that includes
 * the token type name, the lexeme (original text), and the source span.
 *
 * @return A formatted string representation of this token
 */
std::string Token::toString() const {
  return std::format("[{}] \"{}\" at {}", this->getName(), Lexeme,
                     Span.toString());
}

} // namespace phi
