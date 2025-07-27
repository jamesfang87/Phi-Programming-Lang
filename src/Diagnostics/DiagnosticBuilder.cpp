#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

/// Add a primary label (shown with arrow pointer)
DiagnosticBuilder& DiagnosticBuilder::with_primary_label(const SrcSpan& span, std::string message) {
    diagnostic.with_primary_label(span, std::move(message));
    return *this;
}

/// Add a primary label using individual location components
DiagnosticBuilder& DiagnosticBuilder::with_primary_label(const std::string& path,
                                                         const int line,
                                                         const int col,
                                                         std::string message) {
    const SrcLocation loc{path, line, col};
    return with_primary_label(SrcSpan(loc), std::move(message));
}

/// Add a primary label spanning from start to end
DiagnosticBuilder& DiagnosticBuilder::with_primary_span(const std::string& path,
                                                        const int start_line,
                                                        const int start_col,
                                                        const int end_line,
                                                        const int end_col,
                                                        std::string message) {
    const SrcLocation start{path, start_line, start_col};
    const SrcLocation end{path, end_line, end_col};
    return with_primary_label(SrcSpan(start, end), std::move(message));
}

/// Add a secondary label (shown with underline)
DiagnosticBuilder& DiagnosticBuilder::with_secondary_label(const SrcSpan& span,
                                                           std::string message,
                                                           const DiagnosticStyle style) {
    diagnostic.with_secondary_label(span, std::move(message), style);
    return *this;
}

/// Add a secondary label using individual location components
DiagnosticBuilder& DiagnosticBuilder::with_secondary_label(const std::string& path,
                                                           const int line,
                                                           const int col,
                                                           std::string message,
                                                           const DiagnosticStyle style) {
    const SrcLocation loc{path, line, col};
    return with_secondary_label(SrcSpan(loc), std::move(message), style);
}

/// Add a note (additional information)
DiagnosticBuilder& DiagnosticBuilder::with_note(std::string note) {
    diagnostic.with_note(std::move(note));
    return *this;
}

/// Add a help message (suggestions)
DiagnosticBuilder& DiagnosticBuilder::with_help(std::string help) {
    diagnostic.with_help(std::move(help));
    return *this;
}

/// Add a code suggestion for fixing the issue
DiagnosticBuilder& DiagnosticBuilder::with_suggestion(const SrcSpan& span,
                                                      std::string replacement,
                                                      std::string description) {
    diagnostic.with_suggestion(span, std::move(replacement), std::move(description));
    return *this;
}

/// Set an error code (e.g., "E0308")
DiagnosticBuilder& DiagnosticBuilder::with_code(std::string code) {
    diagnostic.with_code(std::move(code));
    return *this;
}

/// Get the built diagnostic (move semantics)
Diagnostic DiagnosticBuilder::build() && { return std::move(diagnostic); }

/// Get the built diagnostic (copy)
[[nodiscard]] Diagnostic DiagnosticBuilder::build() const& { return diagnostic; }

/// Emit the diagnostic immediately using the provided manager
void DiagnosticBuilder::emit(const DiagnosticManager& manager, std::ostream& out) const&& {
    manager.emit(diagnostic, out);
}

/// Emit the diagnostic immediately using the provided manager (copy version)
void DiagnosticBuilder::emit(const DiagnosticManager& manager, std::ostream& out) const& {
    manager.emit(diagnostic, out);
}
} // namespace phi
