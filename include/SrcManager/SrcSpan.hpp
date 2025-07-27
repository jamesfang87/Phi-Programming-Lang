#include "Lexer/Token.hpp"
#include "SrcManager/SrcLocation.hpp"

/// Represents a span of source code with start and end positions
struct SrcSpan {
    SrcLocation start;
    SrcLocation end;

    SrcSpan(SrcLocation start, SrcLocation end)
        : start(std::move(start)),
          end(std::move(end)) {}

    explicit SrcSpan(const SrcLocation& single_pos)
        : start(single_pos),
          end(single_pos) {}

    /// Returns true if this span covers multiple lines
    [[nodiscard]] bool is_multiline() const { return start.line != end.line; }

    /// Returns the number of lines this span covers
    [[nodiscard]] int line_count() const { return end.line - start.line + 1; }
};

// Create a source span from a token
[[nodiscard]] inline SrcSpan span_from_token(const Token& token) {
    return SrcSpan{token.get_start(), token.get_end()};
}

// Create a source span from two tokens (inclusive range)
[[nodiscard]] inline SrcSpan span_from_tokens(const Token& start, const Token& end) {
    return SrcSpan{start.get_start(), end.get_end()};
}
