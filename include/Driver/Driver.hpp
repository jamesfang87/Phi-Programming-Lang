#pragma once

#include <string>

#include "Diagnostics/DiagnosticManager.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// PhiCompiler - Main compiler driver for the Phi programming language
//===----------------------------------------------------------------------===//

/**
 * @brief Main compiler driver for the Phi programming language
 *
 * Orchestrates the complete compilation pipeline from source code to output.
 * Coordinates between lexical analysis, parsing, semantic analysis, and
 * code generation phases while managing diagnostic reporting throughout
 * the compilation process.
 */
class PhiCompiler {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  /**
   * @brief Constructs a Phi compiler instance
   *
   * @param Src Source code content to compile
   * @param Path File path for error reporting and output naming
   * @param DiagnosticsMan Diagnostic system for error/warning reporting
   */
  PhiCompiler(std::string Src, std::string Path,
              std::shared_ptr<DiagnosticManager> DiagnosticsMan);

  ~PhiCompiler();

  //===--------------------------------------------------------------------===//
  // Main Compilation Entry Point
  //===--------------------------------------------------------------------===//

  /**
   * @brief Executes the complete compilation pipeline
   *
   * Performs lexical analysis, parsing, semantic analysis, and code generation
   * in sequence. Reports errors and warnings through the diagnostic manager.
   *
   * @return true if compilation succeeded without errors, false otherwise
   */
  bool compile();

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::string SrcFile; ///< Source code content
  std::string Path;    ///< Source file path
  std::shared_ptr<DiagnosticManager>
      DiagnosticMan; ///< Diagnostic reporting system
};

} // namespace phi
