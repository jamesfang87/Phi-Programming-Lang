#pragma once

#include <string>

struct SrcLocation {
    std::string path;
    int line, col;
};

/// Represents a span of source code with start and end positions
struct SourceSpan {
    SrcLocation start;
    SrcLocation end;

    SourceSpan(SrcLocation start, SrcLocation end)
        : start(std::move(start)),
          end(std::move(end)) {}

    explicit SourceSpan(const SrcLocation& single_pos)
        : start(single_pos),
          end(single_pos) {}

    /// Returns true if this span covers multiple lines
    [[nodiscard]] bool is_multiline() const { return start.line != end.line; }

    /// Returns the number of lines this span covers
    [[nodiscard]] int line_count() const { return end.line - start.line + 1; }
};
