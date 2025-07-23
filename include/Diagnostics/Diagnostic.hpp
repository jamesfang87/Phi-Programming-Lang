#pragma once

#include "SrcLocation.hpp"
#include <algorithm>
#include <cstdint>
#include <optional>
#include <string>
#include <vector>

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

    SourceSpan(const SrcLocation& start, const SrcLocation& end)
        : start(start),
          end(end) {}

    SourceSpan(const SrcLocation& single_pos)
        : start(single_pos),
          end(single_pos) {}

    /// Returns true if this span covers multiple lines
    bool is_multiline() const { return start.line != end.line; }

    /// Returns the number of lines this span covers
    int line_count() const { return end.line - start.line + 1; }
};

/// Represents styling information for diagnostic output
struct DiagnosticStyle {
    enum class Color : uint8_t { Red, Yellow, Blue, Green, Cyan, Magenta, White, Default };

    Color color = Color::Default;
    bool bold = false;
    bool underline = false;

    DiagnosticStyle() = default;
    DiagnosticStyle(Color c, bool bold = false, bool underline = false)
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

    DiagnosticLabel(const SourceSpan& span,
                    std::string message,
                    DiagnosticStyle style = DiagnosticStyle(),
                    bool is_primary = false)
        : span(span),
          message(std::move(message)),
          style(style),
          is_primary(is_primary) {}
};

/// Represents a code suggestion for fixing an issue
struct DiagnosticSuggestion {
    SourceSpan span;
    std::string replacement_text;
    std::string description;

    DiagnosticSuggestion(const SourceSpan& span, std::string replacement, std::string desc)
        : span(span),
          replacement_text(std::move(replacement)),
          description(std::move(desc)) {}
};

/// Main diagnostic class representing a single diagnostic message
class Diagnostic {
public:
    /// Create a new diagnostic with the specified level and message
    Diagnostic(DiagnosticLevel level, std::string message)
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
    DiagnosticLevel level() const { return level_; }
    const std::string& message() const { return message_; }
    const std::vector<DiagnosticLabel>& labels() const { return labels_; }
    const std::vector<std::string>& notes() const { return notes_; }
    const std::vector<std::string>& help_messages() const { return help_messages_; }
    const std::vector<DiagnosticSuggestion>& suggestions() const { return suggestions_; }
    const std::optional<std::string>& code() const { return code_; }

    /// Check if this diagnostic has any primary labels
    bool has_primary_labels() const {
        return std::any_of(labels_.begin(), labels_.end(), [](const DiagnosticLabel& label) {
            return label.is_primary;
        });
    }

    /// Get the primary span (first primary label's span, or first label's span)
    std::optional<SourceSpan> primary_span() const {
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
        return Diagnostic(DiagnosticLevel::Error, std::move(message));
    }

    static Diagnostic warning(std::string message) {
        return Diagnostic(DiagnosticLevel::Warning, std::move(message));
    }

    static Diagnostic note(std::string message) {
        return Diagnostic(DiagnosticLevel::Note, std::move(message));
    }

    static Diagnostic help(std::string message) {
        return Diagnostic(DiagnosticLevel::Help, std::move(message));
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
    static DiagnosticStyle get_style_for_level(DiagnosticLevel level) {
        switch (level) {
            case DiagnosticLevel::Error: return DiagnosticStyle(DiagnosticStyle::Color::Red, true);
            case DiagnosticLevel::Warning:
                return DiagnosticStyle(DiagnosticStyle::Color::Yellow, true);
            case DiagnosticLevel::Note: return DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
            case DiagnosticLevel::Help: return DiagnosticStyle(DiagnosticStyle::Color::Green, true);
        }
        return DiagnosticStyle();
    }

    /// Get the default style for secondary labels
    static DiagnosticStyle get_secondary_style() {
        return DiagnosticStyle(DiagnosticStyle::Color::Cyan);
    }
};

} // namespace phi
