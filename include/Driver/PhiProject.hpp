#pragma once

#include <filesystem>
#include <map>
#include <memory>
#include <string>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "Lexer/Token.hpp"

namespace phi {
namespace fs = std::filesystem;

//===----------------------------------------------------------------------===//
// Compilation Unit
//===----------------------------------------------------------------------===//
struct CompilationUnit {
  std::string Filename;
  std::string Source;
  std::vector<Token> Tokens;

  fs::path ObjectFile;
  fs::path AssemblyFile;
  fs::path LLVMFile;
};

//===----------------------------------------------------------------------===//
// Project Config
//===----------------------------------------------------------------------===//
struct ProjectConfig {
  std::string ProjectName;
  fs::path OutputDir = "build";
  std::vector<fs::path> Sources;
  std::vector<fs::path> IncludePaths;
  int OptimizationLevel = 0;
  bool EmitLLVM = false;
  bool EmitAssembly = false;
  bool CompileOnly = false;
};

//===----------------------------------------------------------------------===//
// Project Loader
//===----------------------------------------------------------------------===//
class PhiProject {
public:
  PhiProject(const fs::path &ProjectFile);

  const auto &getConfig() const { return Config; }
  const auto &getCompilationUnits() const { return Units; }
  auto &getModules() { return Modules; }
  bool registerIntoModule(std::unique_ptr<ModuleDecl> PartialModule);

private:
  ProjectConfig Config;
  std::vector<std::unique_ptr<CompilationUnit>> Units;
  std::map<std::string, std::unique_ptr<ModuleDecl>> Modules;

  void loadConfig(const fs::path &Path);
  void prepareUnits();
};

} // namespace phi
