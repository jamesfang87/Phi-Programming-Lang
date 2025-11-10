/**
 * @file Token.hpp
 * @brief Token definitions and Token class for Phi language lexer
 *
 * Defines the TokenType enumeration and Token class representing lexical units.
 * Provides source location tracking and token properties for compiler pipeline.
 */
#pragma once

#include <string>

#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Token - Represents a lexical token in Phi language
//===----------------------------------------------------------------------===//

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
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  /**
   * @brief Constructs a Token
   * @param start Starting source position of token
   * @param end Ending source position of token
   * @param type Token type classification
   * @param lexeme Original source text of token
   */
  Token(SrcLocation Start, SrcLocation End, const TokenKind Kind,
        std::string lexeme)
      : Start(std::move(Start)), End(std::move(End)), Kind(Kind),
        Lexeme(std::move(lexeme)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const SrcLocation &getStart() const { return Start; }
  [[nodiscard]] const SrcLocation &getEnd() const { return End; }
  [[nodiscard]] TokenKind getKind() const { return Kind; }
  [[nodiscard]] std::string getName() const { return TokenKindToStr(Kind); }
  [[nodiscard]] std::string getLexeme() const { return Lexeme; }

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  /**
   * @brief Generates human-readable token representation
   * @return Formatted string containing:
   *         - Token type
   *         - Lexeme content
   *         - Source location information
   */
  [[nodiscard]] std::string toString() const;

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  SrcLocation Start, End; ///< Source position span (inclusive)
  TokenKind Kind;         ///< Token classification type
  std::string Lexeme;     ///< Original source text content
};

} // namespace phi
