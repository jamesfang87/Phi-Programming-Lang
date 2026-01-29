#include "Driver/PhiProject.hpp"

#include <fstream>
#include <llvm/Support/raw_ostream.h>

namespace phi {

PhiProject::PhiProject(const fs::path &ProjectRoot, bool IsRelease) {
  Config.ProjectRoot = ProjectRoot;
  Config.IsRelease = IsRelease;
  Config.OutputDir = ProjectRoot / ".phi" / (IsRelease ? "release" : "debug");

  fs::path PhiTomlPath = ProjectRoot / "Phi.toml";
  if (!fs::exists(PhiTomlPath)) {
    llvm::errs() << "Error: Phi.toml not found in " << ProjectRoot << "\n";
    std::exit(1);
  }

  loadConfig(PhiTomlPath);
  discoverSources();
  prepareUnits();
}

void PhiProject::loadConfig(const fs::path &PhiTomlPath) {
  std::ifstream File(PhiTomlPath);
  if (!File) {
    llvm::errs() << "Error: Cannot read Phi.toml\n";
    std::exit(1);
  }

  std::string Line;
  while (std::getline(File, Line)) {
    // Parse: name = "project_name"
    if (Line.find("name = ") != std::string::npos) {
      auto Start = Line.find('"');
      auto End = Line.rfind('"');
      if (Start != std::string::npos && End != std::string::npos &&
          Start < End) {
        Config.ProjectName = Line.substr(Start + 1, End - Start - 1);
      }
    }
  }

  if (Config.ProjectName.empty()) {
    Config.ProjectName = "UnnamedProject";
  }
}

void PhiProject::discoverSources() {
  // Discover all .phi files in src/
  fs::path SrcDir = Config.ProjectRoot / "src";

  if (!fs::exists(SrcDir) || !fs::is_directory(SrcDir)) {
    llvm::errs() << "Error: src/ directory not found\n";
    std::exit(1);
  }

  // Recursively find all .phi files
  for (const auto &Entry : fs::recursive_directory_iterator(SrcDir)) {
    if (Entry.is_regular_file() && Entry.path().extension() == ".phi") {
      Config.Sources.push_back(Entry.path());
    }
  }

  if (Config.Sources.empty()) {
    llvm::errs() << "Error: No .phi files found in src/\n";
    std::exit(1);
  }
}

void PhiProject::prepareUnits() {
  for (auto &SrcPath : Config.Sources) {
    auto Unit = std::make_unique<CompilationUnit>();
    Unit->Filename = SrcPath.string();

    if (!fs::exists(SrcPath)) {
      llvm::errs() << "Warning: Source file not found: " << SrcPath << "\n";
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

  // Merge into existing module
  auto &Master = Modules.at(PartialModule->getId());
  Master->addFrom(std::move(*PartialModule));
  return false;
}

} // namespace phi
