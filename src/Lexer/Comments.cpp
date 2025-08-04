#include "Lexer/Lexer.hpp"

namespace phi {

/**
 * @brief Skips over comment text (both line and block comments)
 *
 * This method handles both types of comments supported by Phi:
 * - Line comments: // (skip until end of line)
 * - Block comments: begin with slash-star, end with star-slash
 *
 * The method properly handles:
 * - Line number tracking within multi-line block comments
 * - Nested comment detection (reports error for unclosed block comments)
 * - EOF handling within comments
 *
 * @throws Scanning error for unclosed block comments
 */
void Lexer::skipComment() {
  // first consume the first '/' to decide what to do next
  advanceChar();
  if (matchNext('/')) {
    // skip until we reach the end of line
    while (peekChar() != '\n') {
      advanceChar();
    }
  } else if (matchNext('*')) {
    // Track where the block comment started
    auto startPos = curLexeme;
    auto startLine = lexemeLine;
    int startLineNum = lineNum;

    int depth = 1; // depth for nested comments
    // skip until we reach a depth of 0
    while (!atEOF() && depth > 0) {
      if (peekChar() == '/' && peekNext() == '*') {
        // skip / and *
        advanceChar();
        advanceChar();
        depth++;
      } else if (peekChar() == '*' && peekNext() == '/') {
        // skip * and /
        advanceChar();
        advanceChar();
        depth--;
      } else {
        // increment line number if we see '\n'
        if (peekChar() == '\n') {
          lineNum++;
          curLine = curChar + 1; // Point to start of next line
        }
        advanceChar();
      }
    }

    if (depth > 0) {
      emitUnclosedBlockCommentError(startPos, startLine, startLineNum);
    }
  }
}

} // namespace phi
