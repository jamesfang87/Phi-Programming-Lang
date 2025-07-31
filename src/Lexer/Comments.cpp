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
void Lexer::skip_comment() {
    // first consume the first '/' to decide what to do next
    advance_char();
    if (match_next('/')) {
        // skip until we reach the end of line
        while (peek_char() != '\n') {
            advance_char();
        }
    } else if (match_next('*')) {
        // Track where the block comment started
        auto comment_start_pos = cur_lexeme;
        auto comment_start_line = lexeme_line;
        int comment_start_line_num = line_num;

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
            } else {
                // increment line number if we see '\n'
                if (peek_char() == '\n') {
                    line_num++;
                    cur_line = cur_char + 1; // Point to start of next line
                }
                advance_char();
            }
        }

        if (depth > 0) {
            emit_unclosed_block_comment_error(comment_start_pos,
                                              comment_start_line,
                                              comment_start_line_num);
        }
    }
}

} // namespace phi
