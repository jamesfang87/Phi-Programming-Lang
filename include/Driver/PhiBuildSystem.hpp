#pragma once

//===----------------------------------------------------------------------===//
// PhiBuildSystem.hpp - Build System for Phi (Phase 1-2)
//===----------------------------------------------------------------------===//

#include "Diagnostics/DiagnosticManager.hpp"
#include "Driver/PhiProject.hpp"

#include <filesystem>
#include <llvm/Support/raw_ostream.h>
#include <optional>
#include <string>
#include <vector>

namespace phi {

namespace fs = std::filesystem;

//===----------------------------------------------------------------------===//
// Build Mode
//===----------------------------------------------------------------------===//

enum class BuildMode {
  SingleFile, // phi compile file.phi
  Project,    // phi build
};

//===----------------------------------------------------------------------===//
// Compiler Options
//===----------------------------------------------------------------------===//

struct CompilerOptions {
  BuildMode Mode = BuildMode::Project;
  bool IsRelease = false;
  bool Verbose = false;
  bool DumpTokens = false;
  bool DumpAST = false;

  // Single file mode
  std::optional<fs::path> InputFile;
  std::optional<fs::path> OutputPath;

  // Project mode
  std::optional<fs::path> ProjectRoot;
};

//===----------------------------------------------------------------------===//
// Phi Build System
//===----------------------------------------------------------------------===//

class PhiBuildSystem {
public:
  // Single file compilation
  static bool compileSingleFile(const fs::path &SourceFile,
                                const CompilerOptions &Opts);

  // Project compilation (multi-module)
  static bool buildProject(const CompilerOptions &Opts);

  // Run executable with arguments
  static int run(const fs::path &Executable,
                 const std::vector<std::string> &Args);

  // Project management
  static bool createProject(const std::string &Name);
  static bool initProject();

  // Utilities
  static void clean();

private:
  // Compilation helpers
  static bool compileFile(const fs::path &SourceFile,
                          const fs::path &OutputFile, bool IsRelease,
                          DiagnosticManager &Diags);

  static void compileUnit(CompilationUnit &Unit, DiagnosticManager &Diags);
  static void linkObjects(PhiProject &Project);

  // Project config helpers
  static std::optional<fs::path> findPhiToml(const fs::path &StartDir);
  static std::string getProjectName(const fs::path &PhiTomlPath);
};

} // namespace phi
