#include "Lexer/Token.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Represents a source code span
 *
 * Defines a contiguous region of source code with start and end positions.
 * Used for error highlighting and source context.
 */
struct SrcSpan {
    SrcLocation start; ///< Start position (inclusive)
    SrcLocation end;   ///< End position (inclusive)

    /**
     * @brief Constructs span from positions
     * @param start Start location
     * @param end End location
     */
    SrcSpan(SrcLocation start, SrcLocation end)
        : start(std::move(start)),
          end(std::move(end)) {}

    /**
     * @brief Constructs single-position span
     * @param single_pos Location for single point
     */
    explicit SrcSpan(const SrcLocation& single_pos)
        : start(single_pos),
          end(single_pos) {}

    /**
     * @brief Checks for multi-line span
     * @return true if spans multiple lines
     */
    [[nodiscard]] bool is_multiline() const { return start.line != end.line; }

    /**
     * @brief Calculates line count
     * @return Number of lines covered
     */
    [[nodiscard]] int line_count() const { return end.line - start.line + 1; }
};

/**
 * @brief Creates span from token
 * @param token Source token
 * @return Span covering token
 */
[[nodiscard]] inline SrcSpan span_from_token(const Token& token) {
    return SrcSpan{token.get_start(), token.get_end()};
}

/**
 * @brief Creates span from token range
 * @param start First token
 * @param end Last token
 * @return Span covering token range
 */
[[nodiscard]] inline SrcSpan span_from_tokens(const Token& start, const Token& end) {
    return SrcSpan{start.get_start(), end.get_end()};
}

} // namespace phi
