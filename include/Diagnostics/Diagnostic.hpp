#pragma once

#include <algorithm>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "SrcLocation.hpp"

namespace phi {

/// Represents the severity level of a diagnostic message
enum class DiagnosticLevel : uint8_t {
    Error,   // Red, causes compilation failure
    Warning, // Yellow, compilation continues
    Note,    // Blue, additional information
    Help     // Green, suggestions for fixes
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

/// Represents styling information for diagnostic output
struct DiagnosticStyle {
    enum class Color : uint8_t { Red, Yellow, Blue, Green, Cyan, Magenta, White, Default };

    Color color = Color::Default;
    bool bold = false;
    bool underline = false;

    DiagnosticStyle() = default;
    explicit DiagnosticStyle(const Color c, const bool bold = false, const bool underline = false)
        : color(c),
          bold(bold),
          underline(underline) {}
};

/// Represents a label pointing to a specific source location with a message
struct DiagnosticLabel {
    SourceSpan span;
    std::string message;
    DiagnosticStyle style;
    bool is_primary; // Primary labels get special formatting (arrows vs underlines)

    DiagnosticLabel(SourceSpan span,
                    std::string message,
                    const DiagnosticStyle style = DiagnosticStyle(),
                    const bool is_primary = false)
        : span(std::move(span)),
          message(std::move(message)),
          style(style),
          is_primary(is_primary) {}
};

/// Represents a code suggestion for fixing an issue
struct DiagnosticSuggestion {
    SourceSpan span;
    std::string replacement_text;
    std::string description;

    DiagnosticSuggestion(SourceSpan span, std::string replacement, std::string desc)
        : span(std::move(span)),
          replacement_text(std::move(replacement)),
          description(std::move(desc)) {}
};

/// Main diagnostic class representing a single diagnostic message
class Diagnostic {
public:
    /// Create a new diagnostic with the specified level and message
    Diagnostic(const DiagnosticLevel level, std::string message)
        : level_(level),
          message_(std::move(message)) {}

    /// Add a primary label (gets arrow pointer and special formatting)
    Diagnostic& with_primary_label(const SourceSpan& span, std::string message) {
        labels_.emplace_back(span, std::move(message), get_style_for_level(level_), true);
        return *this;
    }

    /// Add a secondary label (gets underline formatting)
    Diagnostic& with_secondary_label(const SourceSpan& span,
                                     std::string message,
                                     DiagnosticStyle style = DiagnosticStyle()) {
        if (style.color == DiagnosticStyle::Color::Default) {
            style = get_secondary_style();
        }
        labels_.emplace_back(span, std::move(message), style, false);
        return *this;
    }

    /// Add a note (additional information, shown after the main diagnostic)
    Diagnostic& with_note(std::string note) {
        notes_.emplace_back(std::move(note));
        return *this;
    }

    /// Add a help message (suggestions, shown after notes)
    Diagnostic& with_help(std::string help) {
        help_messages_.emplace_back(std::move(help));
        return *this;
    }

    /// Add a code suggestion
    Diagnostic&
    with_suggestion(const SourceSpan& span, std::string replacement, std::string description) {
        suggestions_.emplace_back(span, std::move(replacement), std::move(description));
        return *this;
    }

    /// Set an error code (e.g., "E0308" like Rust)
    Diagnostic& with_code(std::string code) {
        code_ = std::move(code);
        return *this;
    }

    // Getters
    [[nodiscard]] DiagnosticLevel level() const { return level_; }
    [[nodiscard]] const std::string& message() const { return message_; }
    [[nodiscard]] const std::vector<DiagnosticLabel>& labels() const { return labels_; }
    [[nodiscard]] const std::vector<std::string>& notes() const { return notes_; }
    [[nodiscard]] const std::vector<std::string>& help_messages() const { return help_messages_; }
    [[nodiscard]] const std::vector<DiagnosticSuggestion>& suggestions() const {
        return suggestions_;
    }
    [[nodiscard]] const std::optional<std::string>& code() const { return code_; }

    /// Check if this diagnostic has any primary labels
    [[nodiscard]] bool has_primary_labels() const {
        return std::ranges::any_of(labels_,
                                   [](const DiagnosticLabel& label) { return label.is_primary; });
    }

    /// Get the primary span (first primary label's span, or first label's span)
    [[nodiscard]] std::optional<SourceSpan> primary_span() const {
        for (const auto& label : labels_) {
            if (label.is_primary) {
                return label.span;
            }
        }
        if (!labels_.empty()) {
            return labels_[0].span;
        }
        return std::nullopt;
    }

    // Static factory methods for common diagnostic types
    static Diagnostic error(std::string message) {
        return {DiagnosticLevel::Error, std::move(message)};
    }

    static Diagnostic warning(std::string message) {
        return {DiagnosticLevel::Warning, std::move(message)};
    }

    static Diagnostic note(std::string message) {
        return {DiagnosticLevel::Note, std::move(message)};
    }

    static Diagnostic help(std::string message) {
        return {DiagnosticLevel::Help, std::move(message)};
    }

private:
    DiagnosticLevel level_;
    std::string message_;
    std::vector<DiagnosticLabel> labels_;
    std::vector<std::string> notes_;
    std::vector<std::string> help_messages_;
    std::vector<DiagnosticSuggestion> suggestions_;
    std::optional<std::string> code_;

    /// Get the default style for a diagnostic level
    static DiagnosticStyle get_style_for_level(const DiagnosticLevel level) {
        switch (level) {
            case DiagnosticLevel::Error: return DiagnosticStyle(DiagnosticStyle::Color::Red, true);
            case DiagnosticLevel::Warning:
                return DiagnosticStyle(DiagnosticStyle::Color::Yellow, true);
            case DiagnosticLevel::Note: return DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
            case DiagnosticLevel::Help: return DiagnosticStyle(DiagnosticStyle::Color::Green, true);
        }
        return {};
    }

    /// Get the default style for secondary labels
    static DiagnosticStyle get_secondary_style() {
        return DiagnosticStyle(DiagnosticStyle::Color::Cyan);
    }
};

} // namespace phi
