/**
 * @file Lexer.hpp
 * @brief Lexical analyzer (scanner) for the Phi programming language
 *
 * Contains the Lexer class responsible for converting Phi source code
 * from character streams into tokens. Handles lexical elements including:
 * - Keywords and identifiers
 * - Operators and punctuation
 * - String/character literals with escape sequences
 * - Numeric literals (integers and floats)
 * - Comments and whitespace
 */
#pragma once

#include <memory>
#include <string>
#include <vector>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Lexer/Token.hpp"

namespace phi {

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
  /**
   * @brief Constructs a Lexer for given source code
   * @param src Source code string to scan
   * @param path File path for error reporting
   * @param diagnostic_manager Diagnostic system for error reporting
   */
  Lexer(std::string src, std::string path,
        std::shared_ptr<DiagnosticManager> diagnosticsManager)
      : src(std::move(src)), path(std::move(path)),
        diagnosticsManager(std::move(diagnosticsManager)) {
    curChar = this->src.begin();
    curLexeme = this->src.begin();
    curLine = this->src.begin();
    lexemeLine = this->src.begin();
  }

  /**
   * @brief Scans source code to generate tokens
   *
   * Processes the entire source string character-by-character to produce
   * tokens. Handles comments and whitespace appropriately during scanning.
   *
   * @return Pair containing:
   *         - Vector of tokens from source
   *         - Boolean indicating scanning success (false if errors occurred)
   */
  std::pair<std::vector<Token>, bool> scan();

  /**
   * @brief Retrieves source code being scanned
   * @return Copy of source code string
   */
  [[nodiscard]] std::string getSrc() const { return src; }

  /**
   * @brief Retrieves source file path
   * @return Copy of file path string
   */
  [[nodiscard]] std::string getPath() const { return path; }

private:
  std::string src;  ///< Source code being scanned
  std::string path; ///< File path for error reporting
  std::shared_ptr<DiagnosticManager> diagnosticsManager; ///< Diagnostic system

  int line_num = 1;                 ///< Current line number (1-indexed)
  std::string::iterator curChar;    ///< Current character position
  std::string::iterator curLexeme;  ///< Start of current lexeme
  std::string::iterator curLine;    ///< Start of current line
  std::string::iterator lexemeLine; ///< Start of current lexeme's line

  bool insideStr = false; ///< Inside string literal state

  // MAIN SCANNING LOGIC

  /**
   * @brief Scans next token from current position
   * @return Next token in source stream
   */
  Token scanToken();

  // TOKEN PARSING METHODS

  /**
   * @brief Parses numeric literal (integer/float)
   * @return Token representing parsed number
   */
  Token parseNumber();

  /**
   * @brief Parses string literal with escape sequences
   * @return Token representing parsed string
   */
  Token parseStr();

  /**
   * @brief Parses character literal with escape sequences
   * @return Token representing parsed character
   */
  Token parseChar();

  /**
   * @brief Parses identifier/keyword
   * @return Token for identifier or recognized keyword
   */
  Token parseIdentifierOrKw();

  // UTILITY FUNCTIONS

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
  [[nodiscard]] char peekChar() const { return atEOF() ? '\0' : *curChar; }

  /**
   * @brief Peeks next character without advancing
   * @return Next character, or '\0' at/past EOF
   */
  [[nodiscard]] char peekNext() const {
    if (atEOF() || curChar + 1 >= src.end()) {
      return '\0';
    }
    return *(curChar + 1);
  }

  /**
   * @brief Advances to next character
   * @return Character advanced past, or '\0' at EOF
   */
  char advanceChar() {
    if (atEOF())
      return '\0';
    return *curChar++;
  }

  /**
   * @brief Conditionally advances if current character matches
   * @param next Character to match
   * @return true if matched and advanced, false otherwise
   */
  bool matchNext(const char next) {
    if (atEOF() || peekChar() != next) {
      return false;
    }
    ++curChar;
    return true;
  }

  /**
   * @brief Peeks next n characters
   * @param n Number of characters to peek
   * @return String of next n characters (empty if insufficient)
   */
  [[nodiscard]] std::string peekNextN(const int n) const {
    auto temp = curChar;
    for (int i = 0; i < n; ++i) {
      if (atEOF()) {
        return "";
      }
      ++temp;
    }
    return std::string{curChar, temp};
  }

  /**
   * @brief Matches specific character sequence
   * @param next Sequence to match
   * @return true if sequence matched and advanced, false otherwise
   */
  bool matchNextN(const std::string_view next) {
    auto temp = curChar;
    for (const char &c : next) {
      if (atEOF() || *temp != c) {
        return false;
      }
      ++temp;
    }
    curChar = temp;
    return true;
  }

  /**
   * @brief Checks for end of source
   * @return true at EOF, false otherwise
   */
  [[nodiscard]] bool atEOF() const { return curChar >= src.end(); }

  /**
   * @brief Creates token from current lexeme
   * @param type Token type to create
   * @return Token with specified type and current lexeme
   */
  [[nodiscard]] Token makeToken(TokenType type) const {
    int start_col = static_cast<int>(curLexeme - lexemeLine) + 1;
    int end_col = std::distance(curChar, curLine) + 1;

    // TODO: correctly use start line
    int start_line = line_num;
    int end_line = line_num;

    return {
        SrcLocation{.path = this->path, .line = start_line, .col = start_col},
        SrcLocation{.path = this->path, .line = end_line, .col = end_col}, type,
        std::string(curLexeme, curChar)};
  }

  /**
   * @brief Parses escape sequence in literals
   * @return Actual character value of escape sequence
   */
  char parseEscapeSeq();

  /**
   * @brief Parses hexadecimal escape sequence (\xNN)
   * @return Character value from hex digits
   */
  char parseHexEscape();

  // ERROR HANDLING

  /**
   * @brief Reports scanning error using diagnostic system
   * @param message Primary error description
   * @param help_message Optional help message for resolution
   */
  void emitError(std::string_view message, std::string_view helpMessage = "");

  /**
   * @brief Reports unterminated string literal error with ASCII art
   */
  void emitUnterminatedStrError(std::string::iterator startPos,
                                std::string::iterator startLine,
                                int startLineNum);

  /**
   * @brief Reports unterminated character literal error with ASCII art
   */
  void emitUnterminatedCharError();

  /**
   * @brief Reports unclosed block comment error with ASCII art
   */
  void emitUnclosedBlockCommentError(std::string::iterator startPos,
                                     std::string::iterator startLine,
                                     int startLineNum);

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
