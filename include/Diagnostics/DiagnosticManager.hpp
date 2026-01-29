#pragma once

#include <iostream>
#include <map>

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
  bool UseColors = true;               ///< Enable ANSI color codes
  bool ShowLineNumbers = true;         ///< Show line numbers in snippets
  bool ShowSrcContent = true;          ///< Show source code snippets
  int ContextLines = 2;                ///< Lines of context above/below errors
  int MaxLineWidth = 120;              ///< Maximum line width before wrapping
  std::string TabReplacement = "    "; ///< Tab expansion string
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
  explicit DiagnosticManager(DiagnosticConfig Config = DiagnosticConfig());

  //===--------------------------------------------------------------------===//
  // Main Emission Methods
  //===--------------------------------------------------------------------===//

  /**
   * @brief Renders and outputs a single diagnostic
   * @param diagnostic Diagnostic to display
   * @param out Output stream (default: stderr)
   */
  void emit(const Diagnostic &Diag, std::ostream &Out = std::cerr) const;

  /**
   * @brief Renders multiple diagnostics
   * @param diagnostics Diagnostics to display
   * @param out Output stream (default: stderr)
   */
  void emitAll(const std::vector<Diagnostic> &Diags,
               std::ostream &Out = std::cerr) const;

  //===--------------------------------------------------------------------===//
  // Status & Statistics
  //===--------------------------------------------------------------------===//

  /// Gets total error count
  int getErrorCount() const;

  /// Gets total warning count
  int getWarningCount() const;

  /// Checks if any errors were emitted
  bool hasError() const;

  /// Resets error/warning counters
  void resetCounts() const;

  //===--------------------------------------------------------------------===//
  // Configuration Management
  //===--------------------------------------------------------------------===//

  /// Updates visual configuration
  void setConfig(const DiagnosticConfig &NewConfig);

  /// Gets source manager instance
  SrcManager &getSrcManager();

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  class SrcManager Srcs;        ///< Source code access
  DiagnosticConfig Config;      ///< Visual settings
  mutable int ErrorCount = 0;   ///< Total errors emitted
  mutable int WarningCount = 0; ///< Total warnings emitted

  //===--------------------------------------------------------------------===//
  // Main Rendering Methods
  //===--------------------------------------------------------------------===//

  /// Main rendering entry point
  void renderDiagnostic(const Diagnostic &Diag, std::ostream &Out) const;

  /// Renders diagnostic header (level + message)
  void renderDiagnosticHeader(const Diagnostic &Diag, std::ostream &Out) const;

  /// Renders source code snippets with annotations
  void renderSrcSnippets(const Diagnostic &Diag, std::ostream &Out) const;

  //===--------------------------------------------------------------------===//
  // Source Code Rendering
  //===--------------------------------------------------------------------===//

  /// Renders source file snippet with markers
  void renderFileSnippet(const std::string &FilePath,
                         const std::vector<const DiagnosticLabel *> &Labels,
                         std::ostream &Out) const;

  /// Renders line annotations (arrows/underlines)
  void renderLabelsForLine(int LineNum,
                           const std::vector<const DiagnosticLabel *> &Labels,
                           int GutterWidth, const std::string &LineContent,
                           std::ostream &Out) const;

  /// Renders primary label line (^^^ message)
  void renderPrimaryLabelLine(int StartCol, int EndCol,
                              const DiagnosticLabel &Label,
                              std::ostream &Out) const;

  /// Renders secondary label line (~~~~ message)
  void renderSecondaryLabelLine(int StartCol, int EndCol,
                                const DiagnosticLabel &Label,
                                std::ostream &Out) const;

  //===--------------------------------------------------------------------===//
  // Supplementary Information Rendering
  //===--------------------------------------------------------------------===//

  /// Renders supplementary note
  void renderNotes(const std::string &Note, std::ostream &Out) const;

  /// Renders help suggestion
  void renderHelp(const std::string &Help, std::ostream &Out) const;

  /// Renders code suggestion
  void renderSuggestion(const DiagnosticSuggestion &Suggestion,
                        std::ostream &Out) const;

  //===--------------------------------------------------------------------===//
  // Utility Methods
  //===--------------------------------------------------------------------===//

  /// Converts diagnostic level to string ("error", "warning", etc.)
  static std::string diagnosticLevelToString(DiagnosticLevel Level);

  /// Gets default style for diagnostic level
  static DiagnosticStyle getStyleForLevel(DiagnosticLevel Level);

  /// Applies ANSI styling to text if enabled
  std::string formatWithStyle(const std::string &Text,
                              const DiagnosticStyle &Style) const;

  /// Replaces tabs with spaces for consistent alignment
  std::string replaceTabs(std::string_view Line) const;

  /// Groups labels by source file for efficient rendering
  static std::map<std::string, std::vector<const DiagnosticLabel *>>
  groupLabelsByLocation(const std::vector<DiagnosticLabel> &Labels);
};

} // namespace phi
