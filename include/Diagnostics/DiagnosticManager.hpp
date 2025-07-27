#pragma once

#include <iostream>
#include <map>
#include <memory>

#include "Diagnostics/Diagnostic.hpp"
#include "SrcManager/SrcManager.hpp"

namespace phi {

/// Configuration for diagnostic rendering
struct DiagnosticConfig {
    bool use_colors = true;
    bool show_line_numbers = true;
    bool show_source_context = true;
    int context_lines = 2; // Number of lines to show before/after the error
    int max_line_width = 120;
    std::string tab_replacement = "    "; // Replace tabs with spaces for consistent formatting
};

/// Handles formatting and rendering of diagnostics
class DiagnosticManager {
public:
    explicit DiagnosticManager(std::shared_ptr<SrcManager> source_manager,
                               DiagnosticConfig config = DiagnosticConfig());

    /// Emit a diagnostic to the specified stream
    void emit(const Diagnostic& diagnostic, std::ostream& out = std::cerr) const;

    /// Emit multiple diagnostics
    void emit_all(const std::vector<Diagnostic>& diagnostics, std::ostream& out = std::cerr) const;

    /// Get error/warning counts
    int error_count() const;
    int warning_count() const;
    bool has_errors() const;

    /// Reset counters
    void reset_counts() const;

    /// Update configuration
    void set_config(const DiagnosticConfig& config);

    /// Get the source manager
    std::shared_ptr<SrcManager> source_manager() const;

private:
    std::shared_ptr<SrcManager> source_manager_;
    DiagnosticConfig config_;
    mutable int error_count_ = 0;
    mutable int warning_count_ = 0;

    /// Main rendering function
    void render_diagnostic(const Diagnostic& diagnostic, std::ostream& out) const;

    /// Render the diagnostic header (e.g., "error: message")
    void render_diagnostic_header(const Diagnostic& diagnostic, std::ostream& out) const;

    /// Render source code snippets with labels and arrows
    void render_source_snippets(const Diagnostic& diagnostic, std::ostream& out) const;

    /// Render a snippet from a specific file
    void render_file_snippet(const std::string& file_path,
                             const std::vector<const DiagnosticLabel*>& labels,
                             std::ostream& out) const;

    /// Render labels (arrows and underlines) for a specific line
    void render_labels_for_line(int line_num,
                                const std::vector<const DiagnosticLabel*>& labels,
                                int gutter_width,
                                const std::string& line_content,
                                std::ostream& out) const;

    /// Render a primary label (with arrow)
    void render_primary_label_line(int start_col,
                                   int end_col,
                                   const DiagnosticLabel& label,
                                   std::ostream& out) const;

    /// Render a secondary label (with underline)
    void render_secondary_label_line(int start_col,
                                     int end_col,
                                     const DiagnosticLabel& label,
                                     std::ostream& out) const;

    /// Render a note message
    void render_note(const std::string& note, std::ostream& out) const;

    /// Render a help message
    void render_help(const std::string& help, std::ostream& out) const;

    /// Render a suggestion
    void render_suggestion(const DiagnosticSuggestion& suggestion, std::ostream& out) const;

    /// Helper functions
    static std::string diagnostic_level_to_string(DiagnosticLevel level);

    static DiagnosticStyle get_style_for_level(DiagnosticLevel level);

    /// Format text with ANSI color codes
    std::string format_with_style(const std::string& text, const DiagnosticStyle& style) const;

    /// Replace tabs with spaces for consistent formatting
    std::string replace_tabs(std::string_view line) const;

    /// Group labels by file path for rendering
    static std::map<std::string, std::vector<const DiagnosticLabel*>>
    group_labels_by_location(const std::vector<DiagnosticLabel>& labels);
};

} // namespace phi
