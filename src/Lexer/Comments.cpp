#include "Lexer/Lexer.hpp"

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
void Lexer::skip_comment() {
    // first consume the first '/' to decide what to do next
    advance_char();
    if (match_next('/')) {
        // skip until we reach the end of line
        while (peek_char() != '\n') {
            advance_char();
        }
    } else if (match_next('*')) {
        int depth = 1; // depth for nested comments
        // skip until we reach a depth of 0
        while (!reached_eof() && depth > 0) {
            if (peek_char() == '/' && peek_next() == '*') {
                // skip / and *
                advance_char();
                advance_char();
                depth++;
            } else if (peek_char() == '*' && peek_next() == '/') {
                // skip * and /
                advance_char();
                advance_char();
                depth--;
            }

            // increment line number if we see '\n'
            if (peek_char() == '\n') {
                line_num++;
                cur_line = cur_char;
            }
            advance_char();
        }

        if (depth > 0) {
            throw_lexer_error("unclosed block comment", "expected closing `*/` to match this");
        }
    }
}
