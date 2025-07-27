#pragma once

#include <algorithm>
#include <cstdint>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include "SrcManager/SrcSpan.hpp"

namespace phi {

/**
 * @brief Enumeration of diagnostic severity levels
 *
 * Defines the importance and impact of diagnostic messages:
 * - Error: Critical issues that prevent successful compilation.
 *   Displayed in bold red. Compilation terminates after errors.
 * - Warning: Potential problems that don't prevent compilation
 *   but may indicate bugs. Displayed in bold yellow.
 * - Note: Supplementary information that provides context about
 *   errors or warnings. Displayed in bold blue.
 * - Help: Actionable suggestions for resolving issues. Displayed
 *   in bold green.
 */
enum class DiagnosticLevel : uint8_t {
    Error,   ///< Critical errors (bold red)
    Warning, ///< Potential issues (bold yellow)
    Note,    ///< Supplementary info (bold blue)
    Help     ///< Fix suggestions (bold green)
};

/**
 * @brief Visual styling configuration for diagnostic elements
 *
 * Controls text formatting when diagnostics are rendered in
 * terminal environments. Supports color, bold, and underline
 * attributes. When rendered in non-terminal environments,
 * styling is omitted.
 */
struct DiagnosticStyle {
    /// Available text colors for terminal output
    enum class Color : uint8_t {
        Red,     ///< Error highlights
        Yellow,  ///< Warning highlights
        Blue,    ///< Information highlights
        Green,   ///< Success/suggestion highlights
        Cyan,    ///< Secondary elements
        Magenta, ///< Special cases
        White,   ///< Default text
        Default  ///< Terminal default
    };

    Color color = Color::Default; ///< Text color
    bool bold = false;            ///< Bold attribute
    bool underline = false;       ///< Underline attribute

    /// Default constructor creates unstyled text
    DiagnosticStyle() = default;

    /**
     * @brief Constructs styled text configuration
     * @param c Base text color
     * @param bold Enable bold attribute
     * @param underline Enable underline attribute
     */
    explicit DiagnosticStyle(const Color c, const bool bold = false, const bool underline = false)
        : color(c),
          bold(bold),
          underline(underline) {}
};

/**
 * @brief Source-linked annotation for diagnostics
 *
 * Associates a message with a specific region of source code.
 * Primary labels are the main focus of a diagnostic and are
 * marked with arrow pointers (^^^). Secondary labels provide
 * additional context and are underlined.
 */
struct DiagnosticLabel {
    SrcSpan span;          ///< Source code region (start to end)
    std::string message;   ///< Annotation text (1-2 sentences)
    DiagnosticStyle style; ///< Visual rendering style
    bool is_primary;       ///< True for primary focus, false for context

    /**
     * @brief Constructs a source code annotation
     * @param span Source location range to highlight
     * @param message Concise description of the issue
     * @param style Visual styling preferences
     * @param is_primary True if this is the primary issue location
     */
    DiagnosticLabel(SrcSpan span,
                    std::string message,
                    const DiagnosticStyle style = DiagnosticStyle(),
                    const bool is_primary = false)
        : span(std::move(span)),
          message(std::move(message)),
          style(style),
          is_primary(is_primary) {}
};

/**
 * @brief Automated code modification suggestion
 *
 * Provides "quick fix" recommendations that can be automatically
 * applied to resolve issues. Includes the replacement text and
 * a description of why the change is recommended.
 */
struct DiagnosticSuggestion {
    SrcSpan span;                 ///< Source region to replace
    std::string replacement_text; ///< Recommended code
    std::string description;      ///< Rationale for change

    /**
     * @brief Constructs a code modification suggestion
     * @param span Source range to modify
     * @param replacement New code to insert
     * @param desc Explanation of why change is needed
     */
    DiagnosticSuggestion(SrcSpan span, std::string replacement, std::string desc)
        : span(std::move(span)),
          replacement_text(std::move(replacement)),
          description(std::move(desc)) {}
};

/**
 * @brief Comprehensive compiler diagnostic container
 *
 * Represents a complete diagnostic message with:
 * - Primary error description
 * - Source code location markers
 * - Supplementary explanations
 * - Suggested resolutions
 * - Reference codes for documentation
 *
 * Diagnostics are built incrementally using the DiagnosticBuilder
 * fluent interface and rendered by the DiagnosticManager.
 */
class Diagnostic {
public:
    /**
     * @brief Base diagnostic constructor
     * @param level Severity/importance level
     * @param message Primary description (1 sentence summary)
     */
    Diagnostic(const DiagnosticLevel level, std::string message)
        : level(level),
          message(std::move(message)) {}

    /**
     * @brief Adds primary source code marker
     *
     * Primary labels indicate the main location of an issue.
     * They are rendered with arrow pointers:
     *   ^^^ help message
     *
     * @param span Source location range
     * @param message Concise explanation
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic& with_primary_label(const SrcSpan& span, std::string message) {
        labels.emplace_back(span, std::move(message), get_style_for_level(level), true);
        return *this;
    }

    /**
     * @brief Adds secondary source code marker
     *
     * Secondary labels provide additional context. They are
     * rendered with underlines:
     *   ~~~~ note message
     *
     * @param span Source location range
     * @param message Contextual information
     * @param style Custom visual style (defaults to cyan)
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic& with_secondary_label(const SrcSpan& span,
                                     std::string message,
                                     DiagnosticStyle style = DiagnosticStyle()) {
        if (style.color == DiagnosticStyle::Color::Default) {
            style = get_secondary_style();
        }
        labels.emplace_back(span, std::move(message), style, false);
        return *this;
    }

    /**
     * @brief Adds supplementary note
     *
     * Notes appear after source snippets and provide
     * technical details about the issue.
     *
     * @param note Additional explanation (1-2 sentences)
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic& with_note(std::string note) {
        notes.emplace_back(std::move(note));
        return *this;
    }

    /**
     * @brief Adds resolution suggestion
     *
     * Help messages suggest concrete actions to resolve
     * the issue. They appear after notes.
     *
     * @param help Actionable advice (1 sentence)
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic& with_help(std::string help) {
        help_messages.emplace_back(std::move(help));
        return *this;
    }

    /**
     * @brief Adds automated code fix suggestion
     *
     * Suggestions provide specific code modifications that
     * can be automatically applied to fix the issue.
     *
     * @param span Source range to replace
     * @param replacement New code to insert
     * @param description Explanation of change
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic&
    with_suggestion(const SrcSpan& span, std::string replacement, std::string description) {
        suggestions.emplace_back(span, std::move(replacement), std::move(description));
        return *this;
    }

    /**
     * @brief Sets reference code for documentation
     *
     * Error codes (e.g., "E0425") allow users to look up
     * detailed explanations in documentation.
     *
     * @param code Alphanumeric error code
     * @return Diagnostic& Reference for method chaining
     */
    Diagnostic& with_code(std::string code) {
        this->code = std::move(code);
        return *this;
    }

    // ACCESSORS
    /// Gets diagnostic severity level
    [[nodiscard]] DiagnosticLevel get_level() const { return level; }

    /// Gets primary message
    [[nodiscard]] const std::string& get_message() const { return message; }

    /// Gets all source markers
    [[nodiscard]] const std::vector<DiagnosticLabel>& get_labels() const { return labels; }

    /// Gets supplementary notes
    [[nodiscard]] const std::vector<std::string>& get_notes() const { return notes; }

    /// Gets help messages
    [[nodiscard]] const std::vector<std::string>& get_help_messages() const {
        return help_messages;
    }

    /// Gets code suggestions
    [[nodiscard]] const std::vector<DiagnosticSuggestion>& get_suggestions() const {
        return suggestions;
    }

    /// Gets reference code
    [[nodiscard]] const std::optional<std::string>& get_code() const { return code; }

    /// Checks if diagnostic has any primary source markers
    [[nodiscard]] bool has_primary_labels() const {
        return std::ranges::any_of(labels,
                                   [](const DiagnosticLabel& label) { return label.is_primary; });
    }

    /// Gets the most relevant source span (prioritizes primary markers)
    [[nodiscard]] std::optional<SrcSpan> primary_span() const {
        // Search for first primary label
        for (const auto& label : labels) {
            if (label.is_primary) {
                return label.span;
            }
        }
        // Fallback to first label if no primary exists
        if (!labels.empty()) {
            return labels[0].span;
        }
        return std::nullopt;
    }

    // FACTORY METHODS
    /// Creates error-level diagnostic
    static Diagnostic error(std::string message) {
        return {DiagnosticLevel::Error, std::move(message)};
    }

    /// Creates warning-level diagnostic
    static Diagnostic warning(std::string message) {
        return {DiagnosticLevel::Warning, std::move(message)};
    }

    /// Creates note-level diagnostic
    static Diagnostic note(std::string message) {
        return {DiagnosticLevel::Note, std::move(message)};
    }

    /// Creates help-level diagnostic
    static Diagnostic help(std::string message) {
        return {DiagnosticLevel::Help, std::move(message)};
    }

private:
    DiagnosticLevel level;                         ///< Severity classification
    std::string message;                           ///< Primary description
    std::vector<DiagnosticLabel> labels;           ///< Source location markers
    std::vector<std::string> notes;                ///< Technical explanations
    std::vector<std::string> help_messages;        ///< Resolution advice
    std::vector<DiagnosticSuggestion> suggestions; ///< Code modifications
    std::optional<std::string> code;               ///< Reference code

    /// Gets default style based on severity level
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

    /// Gets default style for secondary labels
    static DiagnosticStyle get_secondary_style() {
        return DiagnosticStyle(DiagnosticStyle::Color::Cyan);
    }
};

} // namespace phi
