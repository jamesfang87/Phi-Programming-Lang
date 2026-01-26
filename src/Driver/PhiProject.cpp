#include "Driver/PhiProject.hpp"

#include <fstream>

#include <llvm/Support/raw_ostream.h>

#include <nlohmann/json.hpp>

namespace phi {

PhiProject::PhiProject(const fs::path &ProjectFile) {
  if (!fs::exists(ProjectFile)) {
    llvm::errs() << "Project file not found: " << ProjectFile << "\n";
    std::exit(1);
  }

  loadConfig(ProjectFile);
  prepareUnits();
}

void PhiProject::loadConfig(const fs::path &Path) {
  std::ifstream File(Path);
  nlohmann::json JsonConfig;
  File >> JsonConfig;

  Config.ProjectName = JsonConfig.value("project_name", "UnnamedProject");
  Config.OutputDir = JsonConfig.value("output_dir", "build");

  for (auto &Src : JsonConfig["sources"])
    Config.Sources.push_back(Src.get<std::string>());

  for (auto &Inc : JsonConfig["include_paths"])
    Config.IncludePaths.push_back(Inc.get<std::string>());

  Config.OptimizationLevel = JsonConfig.value("optimization_level", 0);
  Config.EmitLLVM = JsonConfig.value("emit_llvm", false);
  Config.EmitAssembly = JsonConfig.value("emit_assembly", false);
  Config.CompileOnly = JsonConfig.value("compile_only", false);
}

void PhiProject::prepareUnits() {
  for (auto &SrcPath : Config.Sources) {
    auto Unit = std::make_unique<CompilationUnit>();
    Unit->Filename = SrcPath.string();

    if (!fs::exists(SrcPath)) {
      llvm::errs() << "Source file not found: " << SrcPath << "\n";
      continue;
    }

    std::ifstream File(SrcPath);
    Unit->Source.assign((std::istreambuf_iterator<char>(File)),
                        std::istreambuf_iterator<char>());

    Units.push_back(std::move(Unit));
  }
}

bool PhiProject::registerIntoModule(std::unique_ptr<ModuleDecl> PartialModule) {
  if (!Modules.contains(PartialModule->getId())) {
    Modules[PartialModule->getId()] = std::move(PartialModule);
    return true;
  }

  auto &Master = Modules.at(PartialModule->getId());
  Master->addFrom(std::move(*PartialModule));
  return false;
}

} // namespace phi
