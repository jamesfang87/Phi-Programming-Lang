/**
 * @file Token.hpp
 * @brief Token definitions and Token class for Phi language lexer
 *
 * Defines the TokenType enumeration and Token class representing lexical units.
 * Provides source location tracking and token properties for compiler pipeline.
 */
#pragma once

#include <string>

#include "Lexer/TokenType.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Represents a lexical token in Phi language
 *
 * Immutable container holding token metadata including:
 * - Token type classification
 * - Original source text (lexeme)
 * - Precise source location (start and end positions)
 *
 * Provides accessors for token properties and formatted string representation.
 */
class Token {
public:
  /**
   * @brief Constructs a Token
   * @param start Starting source position of token
   * @param end Ending source position of token
   * @param type Token type classification
   * @param lexeme Original source text of token
   */
  Token(SrcLocation start, SrcLocation end, const TokenKind type,
        std::string lexeme)
      : Start(std::move(start)), End(std::move(end)), Type(type),
        Lexeme(std::move(lexeme)) {}

  /**
   * @brief Retrieves token's start position
   * @return Constant reference to start SrcLocation
   */
  [[nodiscard]] const SrcLocation &getStart() const { return Start; }

  /**
   * @brief Retrieves token's end position
   * @return Constant reference to end SrcLocation
   */
  [[nodiscard]] const SrcLocation &getEnd() const { return End; }

  /**
   * @brief Retrieves token type
   * @return TokenType enumeration value
   */
  [[nodiscard]] TokenKind getType() const { return Type; }

  /**
   * @brief Retrieves token type name
   * @return Human-readable type name string
   */
  [[nodiscard]] std::string getName() const { return tyToStr(Type); }

  /**
   * @brief Retrieves original source text
   * @return Lexeme string as it appears in source
   */
  [[nodiscard]] std::string getLexeme() const { return Lexeme; }

  /**
   * @brief Generates human-readable token representation
   * @return Formatted string containing:
   *         - Token type
   *         - Lexeme content
   *         - Source location information
   */
  [[nodiscard]] std::string toString() const;

private:
  SrcLocation Start, End; ///< Source position span (inclusive)
  TokenKind Type;         ///< Token classification type
  std::string Lexeme;     ///< Original source text content
};

} // namespace phi
