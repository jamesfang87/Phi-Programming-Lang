#pragma once

#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticManager.hpp"

namespace phi {

/**
 * @brief Fluent interface for constructing diagnostics
 *
 * Provides a chainable API for incrementally building complex
 * diagnostics with multiple annotations and suggestions.
 *
 * Usage:
 * ```
 * error("Invalid type")
 *     .with_primary_label(span, "Expected integer")
 *     .with_secondary_label(alt_span, "Found string here")
 *     .with_note("Variables must be declared before use")
 *     .emit(manager);
 * ```
 */
class DiagnosticBuilder {
public:
  /**
   * @brief Initializes builder with severity and message
   * @param level Diagnostic importance level
   * @param message Primary description (1 sentence)
   */
  DiagnosticBuilder(const DiagnosticLevel level, std::string message)
      : diagnostic(level, std::move(message)) {}

  /**
   * @brief Adds primary source marker with span
   * @param span Source location range
   * @param message Concise explanation (optional)
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_primary_label(const SrcSpan &span,
                                        std::string message = "");

  /**
   * @brief Adds primary marker with discrete location
   * @param path Source file path
   * @param line Line number (1-indexed)
   * @param col Column number (1-indexed)
   * @param message Concise explanation (optional)
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_primary_label(const std::string &path, const int line,
                                        const int col,
                                        std::string message = "");

  /**
   * @brief Adds primary marker with explicit range
   * @param path Source file path
   * @param start_line Start line (1-indexed)
   * @param start_col Start column (1-indexed)
   * @param end_line End line (1-indexed)
   * @param end_col End column (1-indexed)
   * @param message Concise explanation (optional)
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_primary_span(const std::string &path,
                                       const int start_line,
                                       const int start_col, const int end_line,
                                       const int end_col,
                                       std::string message = "");

  /**
   * @brief Adds secondary source marker with style
   * @param span Source location range
   * @param message Contextual information
   * @param style Custom visual style
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &
  with_secondary_label(const SrcSpan &span, std::string message,
                       const DiagnosticStyle style = DiagnosticStyle());

  /**
   * @brief Adds secondary marker with discrete location
   * @param path Source file path
   * @param line Line number (1-indexed)
   * @param col Column number (1-indexed)
   * @param message Contextual information
   * @param style Custom visual style
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &
  with_secondary_label(const std::string &path, const int line, const int col,
                       std::string message,
                       const DiagnosticStyle style = DiagnosticStyle());

  /**
   * @brief Adds supplementary note
   * @param note Technical explanation (1-2 sentences)
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_note(std::string note);

  /**
   * @brief Adds resolution advice
   * @param help Actionable suggestion (1 sentence)
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_help(std::string help);

  /**
   * @brief Adds automated code fix
   * @param span Source range to modify
   * @param replacement New code to insert
   * @param description Explanation of change
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_suggestion(const SrcSpan &span,
                                     std::string replacement,
                                     std::string description);

  /**
   * @brief Sets documentation reference code
   * @param code Alphanumeric error code (e.g., "E0425")
   * @return DiagnosticBuilder& Reference for chaining
   */
  DiagnosticBuilder &with_code(std::string code);

  /**
   * @brief Finalizes diagnostic (rvalue version)
   * @return Diagnostic Fully constructed diagnostic
   *
   * @note Use when moving the builder (efficient)
   */
  Diagnostic build() &&;

  /**
   * @brief Finalizes diagnostic (lvalue version)
   * @return Diagnostic Copy of constructed diagnostic
   */
  [[nodiscard]] Diagnostic build() const &;

  /**
   * @brief Immediately emits diagnostic
   * @param manager Diagnostic rendering system
   * @param out Output stream (default: stderr)
   *
   * @note Use rvalue version for one-time emission
   */
  void emit(const DiagnosticManager &manager,
            std::ostream &out = std::cerr) const &&;

  /**
   * @brief Immediately emits diagnostic (copy version)
   * @param manager Diagnostic rendering system
   * @param out Output stream (default: stderr)
   */
  void emit(const DiagnosticManager &manager,
            std::ostream &out = std::cerr) const &;

private:
  Diagnostic diagnostic; ///< Diagnostic being constructed
};

// FLUENT FACTORY FUNCTIONS

/**
 * @brief Creates error-level diagnostic builder
 * @param message Primary description
 * @return DiagnosticBuilder Builder instance
 */
inline DiagnosticBuilder error(std::string message) {
  return {DiagnosticLevel::Error, std::move(message)};
}

/**
 * @brief Creates warning-level diagnostic builder
 * @param message Primary description
 * @return DiagnosticBuilder Builder instance
 */
inline DiagnosticBuilder warning(std::string message) {
  return {DiagnosticLevel::Warning, std::move(message)};
}

/**
 * @brief Creates note-level diagnostic builder
 * @param message Supplementary information
 * @return DiagnosticBuilder Builder instance
 */
inline DiagnosticBuilder note(std::string message) {
  return {DiagnosticLevel::Note, std::move(message)};
}

/**
 * @brief Creates help-level diagnostic builder
 * @param message Actionable advice
 * @return DiagnosticBuilder Builder instance
 */
inline DiagnosticBuilder help(std::string message) {
  return {DiagnosticLevel::Help, std::move(message)};
}

// COMMON ERROR HELPERS

/**
 * @brief Creates "expected X found Y" error
 * @param expected Description of expected token/type
 * @param found Description of found token/type
 * @param path Source file path
 * @param line Location line
 * @param col Location column
 * @return DiagnosticBuilder Preconfigured error builder
 */
inline DiagnosticBuilder expected_found_error(const std::string &expected,
                                              const std::string &found,
                                              const std::string &path,
                                              const int line, const int col) {
  return error(std::format("expected {}, found {}", expected, found))
      .with_primary_label(path, line, col,
                          std::format("expected {} here", expected));
}

/**
 * @brief Creates unexpected token error
 * @param token_name Unexpected token string
 * @param path Source file path
 * @param line Location line
 * @param col Location column
 * @return DiagnosticBuilder Preconfigured error builder
 */
inline DiagnosticBuilder unexpected_token_error(const std::string &token_name,
                                                const std::string &path,
                                                const int line, const int col) {
  return error(std::format("unexpected token `{}`", token_name))
      .with_primary_label(path, line, col, "unexpected token");
}

/**
 * @brief Creates missing token error
 * @param expected_token Description of missing token
 * @param path Source file path
 * @param line Location line
 * @param col Location column
 * @return DiagnosticBuilder Preconfigured error builder
 */
inline DiagnosticBuilder missing_token_error(const std::string &expected_token,
                                             const std::string &path,
                                             const int line, const int col) {
  return error(std::format("missing `{}`", expected_token))
      .with_primary_label(path, line, col,
                          std::format("expected `{}` here", expected_token));
}

/**
 * @brief Creates undeclared identifier error
 * @param identifier Unknown identifier name
 * @param path Source file path
 * @param line Location line
 * @param col Location column
 * @return DiagnosticBuilder Preconfigured error builder
 */
inline DiagnosticBuilder
undeclared_identifier_error(const std::string &identifier,
                            const std::string &path, const int line,
                            const int col) {
  return error(std::format("cannot find `{}` in this scope", identifier))
      .with_primary_label(path, line, col, "not found in this scope")
      .with_help("consider declaring the variable before using it");
}

/**
 * @brief Creates type mismatch error
 * @param expected_type Expected type name
 * @param found_type Actual type name
 * @param path Source file path
 * @param line Location line
 * @param col Location column
 * @return DiagnosticBuilder Preconfigured error builder
 */
inline DiagnosticBuilder type_mismatch_error(const std::string &expected_type,
                                             const std::string &found_type,
                                             const std::string &path,
                                             const int line, const int col) {
  return error(std::format("mismatched types"))
      .with_primary_label(
          path, line, col,
          std::format("expected `{}`, found `{}`", expected_type, found_type))
      .with_note(std::format("expected type `{}`", expected_type))
      .with_note(std::format("found type `{}`", found_type));
}

} // namespace phi
