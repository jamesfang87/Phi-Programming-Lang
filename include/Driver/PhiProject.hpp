#pragma once

#include "AST/Nodes/Decl.hpp"
#include "Lexer/Token.hpp"

#include <filesystem>
#include <map>
#include <memory>
#include <string>
#include <vector>

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
  fs::path ProjectRoot;
  fs::path OutputDir;
  std::vector<fs::path> Sources;
  bool IsRelease = false;
};

//===----------------------------------------------------------------------===//
// Project Loader
//===----------------------------------------------------------------------===//

class PhiProject {
public:
  // Load project from Phi.toml
  PhiProject(const fs::path &ProjectRoot, bool IsRelease = false);

  const auto &getConfig() const { return Config; }
  const auto &getCompilationUnits() const { return Units; }
  auto &getModules() { return Modules; }

  // Register parsed module into project
  bool registerIntoModule(std::unique_ptr<ModuleDecl> PartialModule);

private:
  ProjectConfig Config;
  std::vector<std::unique_ptr<CompilationUnit>> Units;
  std::map<std::string, std::unique_ptr<ModuleDecl>> Modules;

  void loadConfig(const fs::path &PhiTomlPath);
  void discoverSources();
  void prepareUnits();
};

} // namespace phi
