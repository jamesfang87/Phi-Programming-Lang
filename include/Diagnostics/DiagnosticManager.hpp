#pragma once

#include <iostream>
#include <map>
#include <memory>

#include "Diagnostics/Diagnostic.hpp"
#include "SrcManager/SrcManager.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// DiagnosticConfig - Configuration for diagnostic rendering
//===----------------------------------------------------------------------===//

/**
 * @brief Configuration for diagnostic rendering
 *
 * Controls visual aspects of diagnostic output:
 * - Color usage
 * - Source context display
 * - Line number visibility
 * - Tab handling
 * - Context line count
 */
struct DiagnosticConfig {
  bool use_colors = true;               ///< Enable ANSI color codes
  bool show_line_numbers = true;        ///< Show line numbers in snippets
  bool show_source_context = true;      ///< Show source code snippets
  int context_lines = 2;                ///< Lines of context above/below errors
  int max_line_width = 120;             ///< Maximum line width before wrapping
  std::string tab_replacement = "    "; ///< Tab expansion string
};

//===----------------------------------------------------------------------===//
// DiagnosticManager - Diagnostic rendering and management system
//===----------------------------------------------------------------------===//

/**
 * @brief Diagnostic rendering and management system
 *
 * Handles:
 * - Formatting diagnostics for terminal output
 * - Managing source code context
 * - Tracking error/warning counts
 * - Applying visual styles
 * - Grouping related diagnostics
 *
 * Uses SrcManager to access source code for context display.
 */
class DiagnosticManager {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  /**
   * @brief Constructs diagnostic manager
   * @param source_manager Source code access system
   * @param config Visual configuration options
   */
  explicit DiagnosticManager(std::shared_ptr<SrcManager> source_manager,
                             DiagnosticConfig config = DiagnosticConfig());

  //===--------------------------------------------------------------------===//
  // Main Emission Methods
  //===--------------------------------------------------------------------===//

  /**
   * @brief Renders and outputs a single diagnostic
   * @param diagnostic Diagnostic to display
   * @param out Output stream (default: stderr)
   */
  void emit(const Diagnostic &diagnostic, std::ostream &out = std::cerr) const;

  /**
   * @brief Renders multiple diagnostics
   * @param diagnostics Diagnostics to display
   * @param out Output stream (default: stderr)
   */
  void emit_all(const std::vector<Diagnostic> &diagnostics,
                std::ostream &out = std::cerr) const;

  //===--------------------------------------------------------------------===//
  // Status & Statistics
  //===--------------------------------------------------------------------===//

  /// Gets total error count
  int error_count() const;

  /// Gets total warning count
  int warning_count() const;

  /// Checks if any errors were emitted
  bool has_errors() const;

  /// Resets error/warning counters
  void reset_counts() const;

  //===--------------------------------------------------------------------===//
  // Configuration Management
  //===--------------------------------------------------------------------===//

  /// Updates visual configuration
  void set_config(const DiagnosticConfig &config);

  /// Gets source manager instance
  std::shared_ptr<SrcManager> source_manager() const;

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::shared_ptr<SrcManager> source_manager_; ///< Source code access
  DiagnosticConfig config_;                    ///< Visual settings
  mutable int error_count_ = 0;                ///< Total errors emitted
  mutable int warning_count_ = 0;              ///< Total warnings emitted

  //===--------------------------------------------------------------------===//
  // Main Rendering Methods
  //===--------------------------------------------------------------------===//

  /// Main rendering entry point
  void render_diagnostic(const Diagnostic &diagnostic, std::ostream &out) const;

  /// Renders diagnostic header (level + message)
  void render_diagnostic_header(const Diagnostic &diagnostic,
                                std::ostream &out) const;

  /// Renders source code snippets with annotations
  void render_source_snippets(const Diagnostic &diagnostic,
                              std::ostream &out) const;

  //===--------------------------------------------------------------------===//
  // Source Code Rendering
  //===--------------------------------------------------------------------===//

  /// Renders source file snippet with markers
  void render_file_snippet(const std::string &file_path,
                           const std::vector<const DiagnosticLabel *> &labels,
                           std::ostream &out) const;

  /// Renders line annotations (arrows/underlines)
  void
  render_labels_for_line(int line_num,
                         const std::vector<const DiagnosticLabel *> &labels,
                         int gutter_width, const std::string &line_content,
                         std::ostream &out) const;

  /// Renders primary label line (^^^ message)
  void render_primary_label_line(int start_col, int end_col,
                                 const DiagnosticLabel &label,
                                 std::ostream &out) const;

  /// Renders secondary label line (~~~~ message)
  void render_secondary_label_line(int start_col, int end_col,
                                   const DiagnosticLabel &label,
                                   std::ostream &out) const;

  //===--------------------------------------------------------------------===//
  // Supplementary Information Rendering
  //===--------------------------------------------------------------------===//

  /// Renders supplementary note
  void render_note(const std::string &note, std::ostream &out) const;

  /// Renders help suggestion
  void render_help(const std::string &help, std::ostream &out) const;

  /// Renders code suggestion
  void render_suggestion(const DiagnosticSuggestion &suggestion,
                         std::ostream &out) const;

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  /// Converts diagnostic level to string ("error", "warning", etc.)
  static std::string diagnostic_level_to_string(DiagnosticLevel level);

  /// Gets default style for diagnostic level
  static DiagnosticStyle get_style_for_level(DiagnosticLevel level);

  /// Applies ANSI styling to text if enabled
  std::string format_with_style(const std::string &text,
                                const DiagnosticStyle &style) const;

  /// Replaces tabs with spaces for consistent alignment
  std::string replace_tabs(std::string_view line) const;

  /// Groups labels by source file for efficient rendering
  static std::map<std::string, std::vector<const DiagnosticLabel *>>
  group_labels_by_location(const std::vector<DiagnosticLabel> &labels);
};

} // namespace phi
