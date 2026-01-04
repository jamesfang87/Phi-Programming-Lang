#pragma once

#include <string>

#include "Lexer/TokenKind.hpp"
#include "SrcManager/SrcLocation.hpp"
#include "SrcManager/SrcSpan.hpp"

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
 */
class Token {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  Token(SrcSpan Span, const TokenKind Kind, std::string Lexeme)
      : Span((std::move(Span))), Kind(Kind), Lexeme(std::move(Lexeme)) {}

  Token(SrcLocation Start, SrcLocation End, const TokenKind::Kind Kind,
        std::string Lexeme)
      : Span(SrcSpan(std::move(Start), std::move(End))), Kind(Kind),
        Lexeme(std::move(Lexeme)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const SrcSpan &getSpan() const { return Span; }
  [[nodiscard]] const SrcLocation &getStart() const { return Span.Start; }
  [[nodiscard]] const SrcLocation &getEnd() const { return Span.End; }
  [[nodiscard]] TokenKind getKind() const { return Kind; }
  [[nodiscard]] std::string getName() const { return Kind.toString(); }
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
  SrcSpan Span;       ///< Span of the Token
  TokenKind Kind;     ///< Token classification type
  std::string Lexeme; ///< Original source text content
};

} // namespace phi
