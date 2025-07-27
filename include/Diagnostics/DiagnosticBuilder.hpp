#pragma once

#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticManager.hpp"

namespace phi {

/// A fluent builder class for constructing diagnostics with a convenient API
/// This class makes it easy to build complex diagnostics in a chain-like manner
class DiagnosticBuilder {
public:
    /// Create a new diagnostic builder with the specified level and message
    DiagnosticBuilder(const DiagnosticLevel level, std::string message)
        : diagnostic(level, std::move(message)) {}

    /// Add a primary label (shown with arrow pointer)
    DiagnosticBuilder& with_primary_label(const SrcSpan& span, std::string message = "");

    /// Add a primary label using individual location components
    DiagnosticBuilder& with_primary_label(const std::string& path,
                                          const int line,
                                          const int col,
                                          std::string message = "");

    /// Add a primary label spanning from start to end
    DiagnosticBuilder& with_primary_span(const std::string& path,
                                         const int start_line,
                                         const int start_col,
                                         const int end_line,
                                         const int end_col,
                                         std::string message = "");

    /// Add a secondary label (shown with underline)
    DiagnosticBuilder& with_secondary_label(const SrcSpan& span,
                                            std::string message,
                                            const DiagnosticStyle style = DiagnosticStyle());

    /// Add a secondary label using individual location components
    DiagnosticBuilder& with_secondary_label(const std::string& path,
                                            const int line,
                                            const int col,
                                            std::string message,
                                            const DiagnosticStyle style = DiagnosticStyle());

    /// Add a note (additional information)
    DiagnosticBuilder& with_note(std::string note);

    /// Add a help message (suggestions)
    DiagnosticBuilder& with_help(std::string help);

    /// Add a code suggestion for fixing the issue
    DiagnosticBuilder&
    with_suggestion(const SrcSpan& span, std::string replacement, std::string description);

    /// Set an error code (e.g., "E0308")
    DiagnosticBuilder& with_code(std::string code);

    /// Get the built diagnostic (move semantics)
    Diagnostic build() &&;

    /// Get the built diagnostic (copy)
    [[nodiscard]] Diagnostic build() const&;

    /// Emit the diagnostic immediately using the provided manager
    void emit(const DiagnosticManager& manager, std::ostream& out = std::cerr) const&&;

    /// Emit the diagnostic immediately using the provided manager (copy version)
    void emit(const DiagnosticManager& manager, std::ostream& out = std::cerr) const&;

private:
    Diagnostic diagnostic;
};

/// Convenience factory functions for creating diagnostic builders

/// Create an error diagnostic builder
inline DiagnosticBuilder error(std::string message) {
    return {DiagnosticLevel::Error, std::move(message)};
}

/// Create a warning diagnostic builder
inline DiagnosticBuilder warning(std::string message) {
    return {DiagnosticLevel::Warning, std::move(message)};
}

/// Create a note diagnostic builder
inline DiagnosticBuilder note(std::string message) {
    return {DiagnosticLevel::Note, std::move(message)};
}

/// Create a help diagnostic builder
inline DiagnosticBuilder help(std::string message) {
    return {DiagnosticLevel::Help, std::move(message)};
}

/// Convenience functions for common parsing errors

/// Create a "expected X found Y" error
inline DiagnosticBuilder expected_found_error(const std::string& expected,
                                              const std::string& found,
                                              const std::string& path,
                                              const int line,
                                              const int col) {
    return error(std::format("expected {}, found {}", expected, found))
        .with_primary_label(path, line, col, std::format("expected {} here", expected));
}

/// Create a "unexpected token" error
inline DiagnosticBuilder unexpected_token_error(const std::string& token_name,
                                                const std::string& path,
                                                const int line,
                                                const int col) {
    return error(std::format("unexpected token `{}`", token_name))
        .with_primary_label(path, line, col, "unexpected token");
}

/// Create a "missing token" error
inline DiagnosticBuilder missing_token_error(const std::string& expected_token,
                                             const std::string& path,
                                             const int line,
                                             const int col) {
    return error(std::format("missing `{}`", expected_token))
        .with_primary_label(path, line, col, std::format("expected `{}` here", expected_token));
}

/// Create an "undeclared identifier" error
inline DiagnosticBuilder undeclared_identifier_error(const std::string& identifier,
                                                     const std::string& path,
                                                     const int line,
                                                     const int col) {
    return error(std::format("cannot find `{}` in this scope", identifier))
        .with_primary_label(path, line, col, "not found in this scope")
        .with_help("consider declaring the variable before using it");
}

/// Create a "type mismatch" error
inline DiagnosticBuilder type_mismatch_error(const std::string& expected_type,
                                             const std::string& found_type,
                                             const std::string& path,
                                             const int line,
                                             const int col) {
    return error(std::format("mismatched types"))
        .with_primary_label(path,
                            line,
                            col,
                            std::format("expected `{}`, found `{}`", expected_type, found_type))
        .with_note(std::format("expected type `{}`", expected_type))
        .with_note(std::format("found type `{}`", found_type));
}

} // namespace phi
