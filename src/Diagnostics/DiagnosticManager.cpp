#include "Diagnostics/DiagnosticManager.hpp"
#include "SrcManager/SrcManager.hpp"

#include <algorithm>
#include <iomanip>

namespace phi {

/**
 * Constructs a diagnostic manager with source manager and configuration.
 */
DiagnosticManager::DiagnosticManager(DiagnosticConfig Config)
    : Srcs(), Config(std::move(Config)) {}

/**
 * Emits a diagnostic to the Output stream.
 *
 * @param diagnostic Diagnostic to emit
 * @param Out Output stream
 *
 * Also tracks error and warning counts for compilation status.
 */
void DiagnosticManager::emit(const Diagnostic &Diag, std::ostream &Out) const {
  renderDiagnostic(Diag, Out);

  // Update error/warning counters
  if (Diag.get_level() == DiagnosticLevel::Error) {
    ErrorCount++;
  } else if (Diag.get_level() == DiagnosticLevel::Warning) {
    WarningCount++;
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
void DiagnosticManager::renderDiagnostic(const Diagnostic &Diag,
                                         std::ostream &Out) const {
  // 1. Render diagnostic header
  renderDiagnosticHeader(Diag, Out);

  // 2. Render source snippets with labels
  if (Config.ShowSrcContent && !Diag.get_labels().empty()) {
    renderSrcSnippets(Diag, Out);
  }

  // 3. Render notes
  for (const auto &Note : Diag.get_notes()) {
    renderNotes(Note, Out);
  }

  // 4. Render help messages
  for (const auto &Help : Diag.get_help_messages()) {
    renderHelp(Help, Out);
  }

  // 5. Render suggestions
  for (const auto &Suggestion : Diag.get_suggestions()) {
    renderSuggestion(Suggestion, Out);
  }

  // New line for separation
  Out << "\n";

  for (auto &Snippet : Diag.get_extra_snippets()) {
    // Wrap snippet in a temporary DiagnosticLabel
    DiagnosticLabel TempLabel(Snippet.first, "");
    // Trick: make the span zero-length so no arrow shows
    TempLabel.span.End = TempLabel.span.Start;

    std::vector<const DiagnosticLabel *> Labels = {&TempLabel};

    // Render a separate snippet block
    Out << Snippet.second << '\n';
    Out << " --> " << Snippet.first.Start.Path << ":"
        << Snippet.first.Start.Line << ":" << Snippet.first.Start.Col << "\n";

    renderFileSnippet(Snippet.first.Start.Path, Labels, Out);
  }
}

/**
 * Renders the diagnostic header with level, code, and message.
 */
void DiagnosticManager::renderDiagnosticHeader(const Diagnostic &Diag,
                                               std::ostream &Out) const {
  const auto LevelStr = diagnosticLevelToString(Diag.get_level());
  const auto Style = getStyleForLevel(Diag.get_level());

  // Format: "error[E0308]: message"
  Out << formatWithStyle(LevelStr, Style);

  if (Diag.get_code()) {
    Out << "[" << *Diag.get_code() << "]";
  }

  Out << ": " << Diag.get_message() << "\n";

  // Show file location for primary span
  if (const auto Span = Diag.primary_span()) {
    Out << " --> " << Span->Start.Path << ":" << Span->Start.Line << ":"
        << Span->Start.Col << "\n";
  }
}

/**
 * Renders source code snippets with all labels grouped by file.
 */
void DiagnosticManager::renderSrcSnippets(const Diagnostic &Diag,
                                          std::ostream &Out) const {
  // Group labels by file
  for (auto GroupedLabels = groupLabelsByLocation(Diag.get_labels());
       const auto &[FilePath, FileLabels] : GroupedLabels) {
    renderFileSnippet(FilePath, FileLabels, Out);
  }
}

/**
 * Renders a file snippet with context lines and labels.
 */
void DiagnosticManager::renderFileSnippet(
    const std::string &FilePath,
    const std::vector<const DiagnosticLabel *> &Labels,
    std::ostream &Out) const {
  if (Labels.empty())
    return;

  // Calculate line range to display with context
  int MinLine = Labels[0]->span.Start.Line;
  int MaxLine = Labels[0]->span.End.Line;

  for (const auto *label : Labels) {
    MinLine = std::min(MinLine, label->span.Start.Line);
    MaxLine = std::max(MaxLine, label->span.End.Line);
  }

  const int StartLine = std::max(1, MinLine - Config.ContextLines);
  const int EndLine =
      std::min(Srcs.getLineCount(FilePath), MaxLine + Config.ContextLines);

  // Calculate gutter width for line numbers
  const int GutterWidth = std::to_string(EndLine).length() + 1;

  // Render empty gutter line
  Out << std::string(GutterWidth, ' ') << " |\n";

  // Render each line with content and labels
  for (int LineNum = StartLine; LineNum <= EndLine; ++LineNum) {
    auto LineContent = Srcs.getLine(FilePath, LineNum);
    if (!LineContent)
      continue;

    // Render line number and content
    if (Config.ShowLineNumbers) {
      Out << std::setw(GutterWidth) << LineNum << " | ";
    }

    std::string FormattedLine = replaceTabs(*LineContent);
    Out << FormattedLine << "\n";

    // Render labels for this line
    renderLabelsForLine(LineNum, Labels, GutterWidth, FormattedLine, Out);
  }

  // Render empty gutter line
  Out << std::string(GutterWidth, ' ') << " |\n";
}

/**
 * Renders labels for a specific line using underlines and arrows.
 */
void DiagnosticManager::renderLabelsForLine(
    const int LineNum, const std::vector<const DiagnosticLabel *> &Labels,
    const int GutterWidth, const std::string &LineContent,
    std::ostream &Out) const {
  std::vector<const DiagnosticLabel *> LineLabels;

  // Collect labels applicable to this line
  for (const auto *Label : Labels) {
    if (LineNum >= Label->span.Start.Line && LineNum <= Label->span.End.Line) {
      LineLabels.push_back(Label);
    }
  }

  if (LineLabels.empty())
    return;

  // Sort by priority (primary first, then by column)
  std::ranges::sort(LineLabels,
                    [](const DiagnosticLabel *A, const DiagnosticLabel *B) {
                      if (A->is_primary != B->is_primary) {
                        return A->is_primary > B->is_primary;
                      }
                      return A->span.Start.Col < B->span.Start.Col;
                    });

  const std::string Gutter = std::string(GutterWidth, ' ') + " | ";

  // Render each label
  for (const auto *Label : LineLabels) {
    if (Label->message.empty()) {
      continue;
    }

    Out << Gutter;

    int StartCol =
        LineNum == Label->span.Start.Line ? Label->span.Start.Col - 1 : 0;
    int EndCol = LineNum == Label->span.End.Line
                     ? Label->span.End.Col - 1
                     : static_cast<int>(LineContent.length() - 1);

    // Clamp to valid column range
    StartCol =
        std::max(0, std::min(StartCol, static_cast<int>(LineContent.length())));
    EndCol = std::max(StartCol,
                      std::min(EndCol, static_cast<int>(LineContent.length())));

    if (Label->is_primary) {
      renderPrimaryLabelLine(StartCol, EndCol, *Label, Out);
    } else {
      renderSecondaryLabelLine(StartCol, EndCol, *Label, Out);
    }

    Out << "\n";
  }
}

/**
 * Renders a primary label line with carets (^^^^) and message.
 */
void DiagnosticManager::renderPrimaryLabelLine(const int StartCol,
                                               const int EndCol,
                                               const DiagnosticLabel &Label,
                                               std::ostream &Out) const {
  const std::string Spaces(StartCol, ' ');
  std::string Arrows;

  if (EndCol > StartCol) {
    Arrows = std::string(EndCol - StartCol, '^');
  } else {
    Arrows = "^";
  }

  Out << Spaces << formatWithStyle(Arrows, Label.style);

  if (!Label.message.empty()) {
    Out << " " << Label.message;
  }
}

/**
 * Renders a secondary label line with tildes (~~~~) and message.
 */
void DiagnosticManager::renderSecondaryLabelLine(const int StartCol,
                                                 const int EndCol,
                                                 const DiagnosticLabel &Label,
                                                 std::ostream &Out) const {
  const std::string Spaces(StartCol, ' ');
  std::string Underline;

  if (EndCol > StartCol) {
    Underline = std::string(EndCol - StartCol, '~');
  } else {
    Underline = "~";
  }

  Out << Spaces << formatWithStyle(Underline, Label.style);

  if (!Label.message.empty()) {
    Out << " " << Label.message;
  }
}

/**
 * Renders a note message.
 */
void DiagnosticManager::renderNotes(const std::string &Note,
                                    std::ostream &Out) const {
  const auto style = DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
  Out << " " << formatWithStyle("note", style) << ": " << Note << "\n";
}

/**
 * Renders a help message.
 */
void DiagnosticManager::renderHelp(const std::string &Help,
                                   std::ostream &Out) const {
  const auto style = DiagnosticStyle(DiagnosticStyle::Color::Green, true);
  Out << " " << formatWithStyle("help", style) << ": " << Help << "\n";
}

/**
 * Renders a code suggestion.
 */
void DiagnosticManager::renderSuggestion(const DiagnosticSuggestion &Suggestion,
                                         std::ostream &Out) const {
  const auto style = DiagnosticStyle(DiagnosticStyle::Color::Green, true);
  Out << " " << formatWithStyle("suggestion", style) << ": "
      << Suggestion.description << "\n";
  // TODO: Show the actual replacement in context
}

/**
 * Converts diagnostic level to string representation.
 */
std::string
DiagnosticManager::diagnosticLevelToString(const DiagnosticLevel Level) {
  switch (Level) {
  case DiagnosticLevel::Error:
    return "error";
  case DiagnosticLevel::Warning:
    return "warning";
  case DiagnosticLevel::Note:
    return "note";
  case DiagnosticLevel::Help:
    return "help";
  }
  return "unknown";
}

/**
 * Gets the display style for a diagnostic level.
 */
DiagnosticStyle
DiagnosticManager::getStyleForLevel(const DiagnosticLevel Level) {
  switch (Level) {
  case DiagnosticLevel::Error:
    return DiagnosticStyle(DiagnosticStyle::Color::Red, true);
  case DiagnosticLevel::Warning:
    return DiagnosticStyle(DiagnosticStyle::Color::Yellow, true);
  case DiagnosticLevel::Note:
    return DiagnosticStyle(DiagnosticStyle::Color::Blue, true);
  case DiagnosticLevel::Help:
    return DiagnosticStyle(DiagnosticStyle::Color::Green, true);
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
std::string
DiagnosticManager::formatWithStyle(const std::string &Text,
                                   const DiagnosticStyle &Style) const {
  if (!Config.UseColors) {
    return Text;
  }

  std::string Result;
  Result += "\033[";
  bool First = true;

  // Handle text attributes
  if (Style.bold) {
    Result += "1";
    First = false;
  }

  if (Style.underline) {
    if (!First)
      Result += ";";
    Result += "4";
    First = false;
  }

  // Handle colors
  if (Style.color != DiagnosticStyle::Color::Default) {
    if (!First)
      Result += ";";

    switch (Style.color) {
    case DiagnosticStyle::Color::Red:
      Result += "31";
      break;
    case DiagnosticStyle::Color::Yellow:
      Result += "33";
      break;
    case DiagnosticStyle::Color::Blue:
      Result += "34";
      break;
    case DiagnosticStyle::Color::Green:
      Result += "32";
      break;
    case DiagnosticStyle::Color::Cyan:
      Result += "36";
      break;
    case DiagnosticStyle::Color::Magenta:
      Result += "35";
      break;
    case DiagnosticStyle::Color::White:
      Result += "37";
      break;
    default:
      break;
    }
  }

  Result += "m" + Text + "\033[0m";
  return Result;
}

/**
 * Replaces tabs with configured replacement string.
 */
std::string DiagnosticManager::replaceTabs(const std::string_view line) const {
  std::string result;
  for (const char c : line) {
    if (c == '\t') {
      result += Config.TabReplacement;
    } else {
      result += c;
    }
  }
  return result;
}

/**
 * Groups diagnostic labels by their source file path.
 */
std::map<std::string, std::vector<const DiagnosticLabel *>>
DiagnosticManager::groupLabelsByLocation(
    const std::vector<DiagnosticLabel> &labels) {
  std::map<std::string, std::vector<const DiagnosticLabel *>> Grouped;

  for (const auto &Label : labels) {
    Grouped[Label.span.Start.Path].push_back(&Label);
  }

  return Grouped;
}

// Public interface methods
int DiagnosticManager::getErrorCount() const { return ErrorCount; }
int DiagnosticManager::getWarningCount() const { return WarningCount; }
bool DiagnosticManager::hasError() const { return ErrorCount > 0; }
void DiagnosticManager::resetCounts() const {
  ErrorCount = 0;
  WarningCount = 0;
}
void DiagnosticManager::setConfig(const DiagnosticConfig &NewConfig) {
  Config = NewConfig;
}

SrcManager &DiagnosticManager::getSrcManager() { return Srcs; }

void DiagnosticManager::emitAll(const std::vector<Diagnostic> &Diags,
                                std::ostream &Out) const {
  for (const auto &Diag : Diags) {
    emit(Diag, Out);
    Out << "\n"; // Add spacing between diagnostics
  }
}

} // namespace phi
