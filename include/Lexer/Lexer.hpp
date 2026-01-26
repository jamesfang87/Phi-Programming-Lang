#pragma once

#include <string>
#include <vector>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenKind.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Lexer - Lexical analyzer for the Phi programming language
//===----------------------------------------------------------------------===//

/**
 * @brief Lexical analyzer for the Phi programming language
 *
 * Converts source code into tokens while maintaining precise source location
 * information. Handles all lexical analysis aspects including:
 * - Comprehensive token recognition
 * - Escape sequence processing in literals
 * - Numeric literal parsing with validation
 * - Comment skipping (both line and block styles)
 * - Detailed error reporting with source positions
 *
 * Maintains line/column tracking throughout scanning for accurate error
 * locations.
 */
class Lexer {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  /**
   * @brief Constructs a Lexer for given source code
   * @param src Source code string to scan
   * @param path File path for error reporting
   * @param diagnostic_manager Diagnostic system for error reporting
   */
  Lexer(std::string Src, std::string Path, DiagnosticManager *Diags)
      : Src(std::move(Src)), Path(std::move(Path)), Diags(std::move(Diags)) {
    CurChar = this->Src.begin();
    CurLexeme = this->Src.begin();
    CurLine = this->Src.begin();
    LexemeLine = this->Src.begin();
  }

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  /**
   * @brief Scans source code to generate tokens
   *
   * Processes the entire source string character-by-character to produce
   * tokens.
   *
   * @return Pair containing:
   *         - Vector of tokens from source
   *         - Boolean indicating scanning success (false if errors
   * occurred)
   */
  std::vector<Token> scan();

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] const std::string &getSrc() const { return Src; }
  [[nodiscard]] const std::string &getPath() const { return Path; }

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::string Src;          ///< Source code being scanned
  std::string Path;         ///< File path for error reporting
  DiagnosticManager *Diags; ///< Diagnostic system

  int LineNum = 1;                  ///< Current line number (1-indexed)
  std::string::iterator CurChar;    ///< Current character position
  std::string::iterator CurLexeme;  ///< Start of current lexeme
  std::string::iterator CurLine;    ///< Start of current line
  std::string::iterator LexemeLine; ///< Start of current lexeme's line

  bool InsideStr = false; ///< Inside string literal state

  //===--------------------------------------------------------------------===//
  // Main Scanning Logic
  //===--------------------------------------------------------------------===//

  /**
   * @brief Scans next token from current position
   * @return Next token in source stream
   */
  Token scanToken();

  //===--------------------------------------------------------------------===//
  // Token Parsing Methods
  //===--------------------------------------------------------------------===//

  Token parseNumber();
  Token parseString();
  Token parseChar();
  Token parseIdentifierOrKw();

  //===--------------------------------------------------------------------===//
  // Utility Functions
  //===--------------------------------------------------------------------===//

  /**
   * @brief Skips comment content
   *
   * Handles both single-line and block comments.
   * Updates line tracking when encountering newlines in comments.
   */
  void skipComment();

  /**
   * @brief Peeks current character without advancing
   * @return Current character, or '\0' at EOF
   */
  [[nodiscard]] char peekChar() const { return atEOF() ? '\0' : *CurChar; }

  /**
   * @brief Peeks next character without advancing
   * @return Next character, or '\0' at/past EOF
   */
  [[nodiscard]] char peekNext() const {
    if (atEOF() || CurChar + 1 >= Src.end()) {
      return '\0';
    }
    return *(CurChar + 1);
  }

  /**
   * @brief Advances to next character
   * @return Character advanced past, or '\0' at EOF
   */
  char advanceChar() {
    if (atEOF())
      return '\0';
    return *CurChar++;
  }

  /**
   * @brief Conditionally advances if current character matches
   * @param next Character to match
   * @return true if matched and advanced, false otherwise
   */
  bool matchNext(const char Next) {
    if (atEOF() || peekChar() != Next) {
      return false;
    }
    ++CurChar;
    return true;
  }

  /**
   * @brief Peeks next n characters
   * @param n Number of characters to peek
   * @return String of next n characters (empty if insufficient)
   */
  [[nodiscard]] std::string peekNextN(const int N) const {
    auto Temp = CurChar;
    for (int i = 0; i < N; ++i) {
      if (atEOF()) {
        return "";
      }
      ++Temp;
    }
    return std::string{CurChar, Temp};
  }

  /**
   * @brief Matches specific character sequence
   * @param next Sequence to match
   * @return true if sequence matched and advanced, false otherwise
   */
  bool matchNextN(const std::string_view Next) {
    auto Temp = CurChar;
    for (const char &C : Next) {
      if (atEOF() || *Temp != C) {
        return false;
      }
      ++Temp;
    }
    CurChar = Temp;
    return true;
  }

  /**
   * @brief Checks for end of source
   * @return true at EOF, false otherwise
   */
  [[nodiscard]] bool atEOF() const { return CurChar >= Src.end(); }

  /**
   * @brief Creates token from current lexeme
   * @param type Token type to create
   * @return Token with specified type and current lexeme
   */
  [[nodiscard]] Token makeToken(TokenKind::Kind Kind) const {
    int StartCol = static_cast<int>(CurLexeme - LexemeLine) + 1;
    int EndCol = std::distance(CurLine, CurChar) + 1;

    // TODO: correctly use start line
    int StartLine = LineNum;
    int EndLine = LineNum;

    return {SrcLocation{.Path = this->Path, .Line = StartLine, .Col = StartCol},
            SrcLocation{.Path = this->Path, .Line = EndLine, .Col = EndCol},
            TokenKind(Kind), std::string(CurLexeme, CurChar)};
  }

  //===--------------------------------------------------------------------===//
  // Escape Sequence Processing
  //===--------------------------------------------------------------------===//

  char parseEscapeSeq();
  char parseHexEscape();

  //===--------------------------------------------------------------------===//
  // Error Handling
  //===--------------------------------------------------------------------===//

  /**
   * @brief Reports scanning error using diagnostic system
   * @param message Primary error description
   * @param help_message Optional help message for resolution
   */
  void emitError(std::string_view Msg, std::string_view HelpMsg = "");

  /**
   * @brief Reports unterminated string literal error
   */
  void emitUnterminatedStrError(std::string::iterator StartPos,
                                std::string::iterator StartLine,
                                int StartLineNum);

  /**
   * @brief Reports unterminated character literal error with ASCII art
   */
  void emitUnterminatedCharError();

  /**
   * @brief Reports unclosed block comment error with ASCII art
   */
  void emitUnclosedBlockCommentError(std::string::iterator StartPos,
                                     std::string::iterator StartLine,
                                     int StartLineNum);

  /**
   * @brief Gets current source location for error reporting
   * @return SrcLocation representing current lexeme position
   */
  [[nodiscard]] SrcLocation getCurLocation() const;

  /**
   * @brief Gets current source span for error reporting
   * @return SrcSpan representing current lexeme range
   */
  [[nodiscard]] SrcSpan getCurSpan() const;
};

} // namespace phi
