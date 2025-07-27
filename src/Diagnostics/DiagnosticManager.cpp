#include "Diagnostics/DiagnosticManager.hpp"

#include <algorithm>
#include <iomanip>
#include <print>

namespace phi {

/**
 * Constructs a diagnostic manager with source manager and configuration.
 */
DiagnosticManager::DiagnosticManager(std::shared_ptr<SrcManager> source_manager,
                                     DiagnosticConfig config)
    : source_manager_(std::move(source_manager)),
      config_(std::move(config)) {}

/**
 * Emits a diagnostic to the output stream.
 *
 * @param diagnostic Diagnostic to emit
 * @param out Output stream
 *
 * Also tracks error and warning counts for compilation status.
 */
void DiagnosticManager::emit(const Diagnostic& diagnostic, std::ostream& out) const {
    render_diagnostic(diagnostic, out);

    // Update error/warning counters
    if (diagnostic.get_level() == DiagnosticLevel::Error) {
        error_count_++;
    } else if (diagnostic.get_level() == DiagnosticLevel::Warning) {
        warning_count_++;
    }
}

/**
 * Renders a complete diagnostic with all components:
 * 1. Header with error code and message
 * 2. Source context with labels
 * 3. Additional notes
 * 4. Help messages
 * 5. Code suggestions
 */
void DiagnosticManager::render_diagnostic(const Diagnostic& diagnostic, std::ostream& out) const {
    // 1. Render diagnostic header
    render_diagnostic_header(diagnostic, out);

    // 2. Render source snippets with labels
    if (config_.show_source_context && !diagnostic.get_labels().empty()) {
        render_source_snippets(diagnostic, out);
    }

    // 3. Render notes
    for (const auto& note : diagnostic.get_notes()) {
        render_note(note, out);
    }

    // 4. Render help messages
    for (const auto& help : diagnostic.get_help_messages()) {
        render_help(help, out);
    }

    // 5. Render suggestions
    for (const auto& suggestion : diagnostic.get_suggestions()) {
        render_suggestion(suggestion, out);
    }
}

/**
 * Renders the diagnostic header with level, code, and message.
 */
void DiagnosticManager::render_diagnostic_header(const Diagnostic& diagnostic,
                                                 std::ostream& out) const {
    const auto level_str = diagnostic_level_to_string(diagnostic.get_level());
    const auto style = get_style_for_level(diagnostic.get_level());

    // Format: "error[E0308]: message"
    out << format_with_style(level_str, style);

    if (diagnostic.get_code()) {
        out << "[" << *diagnostic.get_code() << "]";
    }

    out << ": " << diagnostic.get_message() << "\n";

    // Show file location for primary span
    if (const auto span = diagnostic.primary_span()) {
        out << " --> " << span->start.path << ":" << span->start.line << ":" << span->start.col
            << "\n";
    }
}

/**
 * Renders source code snippets with all labels grouped by file.
 */
void DiagnosticManager::render_source_snippets(const Diagnostic& diagnostic,
                                               std::ostream& out) const {
    // Group labels by file
    for (auto grouped_labels = group_labels_by_location(diagnostic.get_labels());
         const auto& [file_path, file_labels] : grouped_labels) {
        render_file_snippet(file_path, file_labels, out);
    }
}

/**
 * Renders a file snippet with context lines and labels.
 */
void DiagnosticManager::render_file_snippet(const std::string& file_path,
                                            const std::vector<const DiagnosticLabel*>& labels,
                                            std::ostream& out) const {
    if (labels.empty()) return;

    // Calculate line range to display with context
    int min_line = labels[0]->span.start.line;
    int max_line = labels[0]->span.end.line;

    for (const auto* label : labels) {
        min_line = std::min(min_line, label->span.start.line);
        max_line = std::max(max_line, label->span.end.line);
    }

    const int start_line = std::max(1, min_line - config_.context_lines);
    const int end_line =
        std::min(source_manager_->get_line_count(file_path), max_line + config_.context_lines);

    // Calculate gutter width for line numbers
    const int gutter_width = std::to_string(end_line).length() + 1;

    // Render empty gutter line
    out << std::string(gutter_width, ' ') << " |\n";

    // Render each line with content and labels
    for (int line_num = start_line; line_num <= end_line; ++line_num) {
        auto line_content = source_manager_->get_line(file_path, line_num);
        if (!line_content) continue;

        // Render line number and content
        if (config_.show_line_numbers) {
            out << std::setw(gutter_width) << line_num << " | ";
        }

        std::string formatted_line = replace_tabs(*line_content);
        out << formatted_line << "\n";

        // Render labels for this line
        render_labels_for_line(line_num, labels, gutter_width, formatted_line, out);
    }

    // Render empty gutter line
    out << std::string(gutter_width, ' ') << " |\n";
}

/**
 * Renders labels for a specific line using underlines and arrows.
 */
void DiagnosticManager::render_labels_for_line(const int line_num,
                                               const std::vector<const DiagnosticLabel*>& labels,
                                               const int gutter_width,
                                               const std::string& line_content,
                                               std::ostream& out) const {
    std::vector<const DiagnosticLabel*> line_labels;

    // Collect labels applicable to this line
    for (const auto* label : labels) {
        if (line_num >= label->span.start.line && line_num <= label->span.end.line) {
            line_labels.push_back(label);
        }
    }

    if (line_labels.empty()) return;

    // Sort by priority (primary first, then by column)
    std::ranges::sort(line_labels, [](const DiagnosticLabel* a, const DiagnosticLabel* b) {
        if (a->is_primary != b->is_primary) {
            return a->is_primary > b->is_primary;
        }
        return a->span.start.col < b->span.start.col;
    });

    const std::string gutter = std::string(gutter_width, ' ') + " | ";

    // Render each label
    for (const auto* label : line_labels) {
        out << gutter;

        int start_col = line_num == label->span.start.line ? label->span.start.col - 1 : 0;
        int end_col = line_num == label->span.end.line
                          ? label->span.end.col - 1
                          : static_cast<int>(line_content.length() - 1);

        // Clamp to valid column range
        start_col = std::max(0, std::min(start_col, static_cast<int>(line_content.length())));
        end_col = std::max(start_col, std::min(end_col, static_cast<int>(line_content.length())));

        if (label->is_primary) {
            render_primary_label_line(start_col, end_col, *label, out);
        } else {
            render_secondary_label_line(start_col, end_col, *label, out);
        }

        out << "\n";
    }
}

/**
 * Renders a primary label line with carets (^^^^) and message.
 */
void DiagnosticManager::render_primary_label_line(const int start_col,
                                                  const int end_col,
                                                  const DiagnosticLabel& label,
                                                  std::ostream& out) const {
    const std::string spaces(start_col, ' ');
    std::string arrows;

    if (end_col > start_col) {
        arrows = std::string(end_col - start_col, '^');
    } else {
        arrows = "^";
    }

    out << spaces << format_with_style(arrows, label.style);

    if (!label.message.empty()) {
        out << " " << label.message;
    }
}

/**
 * Renders a secondary label line with tildes (~~~~) and message.
 */
void DiagnosticManager::render_secondary_label_line(const int start_col,
                                                    const int end_col,
                                                    const DiagnosticLabel& label,
                                                    std::ostream& out) const {
    const std::string spaces(start_col, ' ');
    std::string underline;

    if (end_col > start_col) {
        underline = std::string(end_col - start_col, '~');
    } else {
        underline = "~";
    }

    out << spaces << format_with_style(underline, label.style);

    if (!label.message.empty()) {
        out << " " << label.message;
    }
}

/**
 * Renders a note message.
 */
void DiagnosticManager::render_note(const std::string& note, std::ostream& out) const {
    const auto style = DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
    out << " " << format_with_style("note", style) << ": " << note << "\n";
}

/**
 * Renders a help message.
 */
void DiagnosticManager::render_help(const std::string& help, std::ostream& out) const {
    const auto style = DiagnosticStyle(DiagnosticStyle::Color::Green, true);
    out << " " << format_with_style("help", style) << ": " << help << "\n";
}

/**
 * Renders a code suggestion.
 */
void DiagnosticManager::render_suggestion(const DiagnosticSuggestion& suggestion,
                                          std::ostream& out) const {
    const auto style = DiagnosticStyle(DiagnosticStyle::Color::Green, true);
    out << " " << format_with_style("suggestion", style) << ": " << suggestion.description << "\n";
    // TODO: Show the actual replacement in context
}

/**
 * Converts diagnostic level to string representation.
 */
std::string DiagnosticManager::diagnostic_level_to_string(const DiagnosticLevel level) {
    switch (level) {
        case DiagnosticLevel::Error: return "error";
        case DiagnosticLevel::Warning: return "warning";
        case DiagnosticLevel::Note: return "note";
        case DiagnosticLevel::Help: return "help";
    }
    return "unknown";
}

/**
 * Gets the display style for a diagnostic level.
 */
DiagnosticStyle DiagnosticManager::get_style_for_level(const DiagnosticLevel level) {
    switch (level) {
        case DiagnosticLevel::Error: return DiagnosticStyle(DiagnosticStyle::Color::Red, true);
        case DiagnosticLevel::Warning: return DiagnosticStyle(DiagnosticStyle::Color::Yellow, true);
        case DiagnosticLevel::Note: return DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
        case DiagnosticLevel::Help: return DiagnosticStyle(DiagnosticStyle::Color::Green, true);
    }
    return DiagnosticStyle();
}

/**
 * Formats text with ANSI color codes based on style settings.
 *
 * @param text Text to format
 * @param style Styling instructions
 * @return Formatted string (or plain text if colors disabled)
 */
std::string DiagnosticManager::format_with_style(const std::string& text,
                                                 const DiagnosticStyle& style) const {
    if (!config_.use_colors) {
        return text;
    }

    std::string result;
    result += "\033[";
    bool first = true;

    // Handle text attributes
    if (style.bold) {
        result += "1";
        first = false;
    }

    if (style.underline) {
        if (!first) result += ";";
        result += "4";
        first = false;
    }

    // Handle colors
    if (style.color != DiagnosticStyle::Color::Default) {
        if (!first) result += ";";

        switch (style.color) {
            case DiagnosticStyle::Color::Red: result += "31"; break;
            case DiagnosticStyle::Color::Yellow: result += "33"; break;
            case DiagnosticStyle::Color::Blue: result += "34"; break;
            case DiagnosticStyle::Color::Green: result += "32"; break;
            case DiagnosticStyle::Color::Cyan: result += "36"; break;
            case DiagnosticStyle::Color::Magenta: result += "35"; break;
            case DiagnosticStyle::Color::White: result += "37"; break;
            default: break;
        }
    }

    result += "m" + text + "\033[0m";
    return result;
}

/**
 * Replaces tabs with configured replacement string.
 */
std::string DiagnosticManager::replace_tabs(const std::string_view line) const {
    std::string result;
    for (const char c : line) {
        if (c == '\t') {
            result += config_.tab_replacement;
        } else {
            result += c;
        }
    }
    return result;
}

/**
 * Groups diagnostic labels by their source file path.
 */
std::map<std::string, std::vector<const DiagnosticLabel*>>
DiagnosticManager::group_labels_by_location(const std::vector<DiagnosticLabel>& labels) {
    std::map<std::string, std::vector<const DiagnosticLabel*>> grouped;

    for (const auto& label : labels) {
        grouped[label.span.start.path].push_back(&label);
    }

    return grouped;
}

// Public interface methods
int DiagnosticManager::error_count() const { return error_count_; }
int DiagnosticManager::warning_count() const { return warning_count_; }
bool DiagnosticManager::has_errors() const { return error_count_ > 0; }
void DiagnosticManager::reset_counts() const {
    error_count_ = 0;
    warning_count_ = 0;
}
void DiagnosticManager::set_config(const DiagnosticConfig& config) { config_ = config; }
std::shared_ptr<SrcManager> DiagnosticManager::source_manager() const { return source_manager_; }

void DiagnosticManager::emit_all(const std::vector<Diagnostic>& diagnostics,
                                 std::ostream& out) const {
    for (const auto& diag : diagnostics) {
        emit(diag, out);
        out << "\n"; // Add spacing between diagnostics
    }
}

} // namespace phi
