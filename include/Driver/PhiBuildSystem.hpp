#pragma once
//===----------------------------------------------------------------------===//
// PhiBuildSystem.cpp - Modern C++ Project-Based Build System for Phi
//===----------------------------------------------------------------------===//

#include <filesystem>
#include <optional>

#include <llvm/Support/raw_ostream.h>

#include "Diagnostics/DiagnosticManager.hpp"
#include "Driver/PhiProject.hpp"

namespace phi {
namespace fs = std::filesystem;

//===----------------------------------------------------------------------===//
// Compiler Options
//===----------------------------------------------------------------------===//
struct CompilerOptions {
  bool EmitLLVM = false;
  bool EmitAssembly = false;
  bool CompileOnly = false;
  bool Verbose = false;
  bool DumpTokens = false;
  bool DumpAST = false;
  int OptimizationLevel = 0;
};

//===----------------------------------------------------------------------===//
// Phi Build System
//===----------------------------------------------------------------------===//
class PhiBuildSystem {
public:
  PhiBuildSystem(int argc, char **argv)
      : Project([&] {
          std::filesystem::path ProjectPath;
          Options = parseCommandLine(argc, argv, ProjectPath);
          return PhiProject(ProjectPath);
        }()) {}

  int build();

private:
  PhiProject Project;
  CompilerOptions Options;
  DiagnosticManager Diags;

  CompilerOptions parseCommandLine(int argc, char **argv,
                                   fs::path &ProjectPath);
  std::optional<fs::path> findProjectFile(const fs::path &StartDir);
  void compileUnit(CompilationUnit &Unit);
  void linkObjects();
};

} // namespace phi
