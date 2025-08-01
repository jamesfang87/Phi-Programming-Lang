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
 * the token type name, the lexeme (original text), and the source location.
 * This method is primarily used for debugging output and compiler diagnostics.
 *
 * The format is: [TOKEN_TYPE] "lexeme" at line N, column M
 *
 * @return A formatted string representation of this token
 */
std::string Token::to_string() const {
    auto [path, line, col] = start;
    return std::format("[{}] \"{}\" at {}:{}:{}", this->get_name(), lexeme, path, line, col);
}

} // namespace phi
