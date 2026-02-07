#include "Driver/PhiBuildSystem.hpp"

#include <fstream>
#include <print>

#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "Sema/TypeInference/Inferencer.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Single File Compilation
//===----------------------------------------------------------------------===//

bool PhiBuildSystem::compileSingleFile(const fs::path &SourceFile,
                                       const CompilerOptions &Opts) {
  if (!fs::exists(SourceFile)) {
    llvm::errs() << "Error: Source file not found: " << SourceFile << "\n";
    return false;
  }

  // Determine output path
  fs::path OutputPath;
  if (Opts.OutputPath.has_value()) {
    OutputPath = Opts.OutputPath.value();
  } else {
    // Default: same name as input without extension
    OutputPath = SourceFile.stem();
  }

  if (Opts.Verbose) {
    llvm::outs() << "[Phi] Compiling: " << SourceFile << "\n";
    llvm::outs() << "[Phi] Output: " << OutputPath << "\n";
  }

  DiagnosticManager Diags;
  return compileFile(SourceFile, OutputPath, Opts.IsRelease, Diags);
}

//===----------------------------------------------------------------------===//
// Project Compilation (Multi-Module)
//===----------------------------------------------------------------------===//

bool PhiBuildSystem::buildProject(const CompilerOptions &Opts) {
  // Find project root
  fs::path ProjectRoot = Opts.ProjectRoot.value_or(fs::current_path());

  // Check for phi.toml
  auto PhiTomlPath = findPhiToml(ProjectRoot);
  if (!PhiTomlPath.has_value()) {
    llvm::errs()
        << "Error: No phi.toml found in current directory or parents\n";
    llvm::errs()
        << "Run 'phi init' to create one, or 'phi new <n>' for a new project\n";
    return false;
  }

  ProjectRoot = PhiTomlPath.value().parent_path();

  if (Opts.Verbose) {
    llvm::outs() << "[Phi] Project root: " << ProjectRoot << "\n";
  }

  // Create and load project (discovers all .phi files in src/)
  PhiProject Project(ProjectRoot, Opts.IsRelease);

  if (Opts.Verbose) {
    llvm::outs() << "[Phi] Project: " << Project.getConfig().ProjectName
                 << "\n";
    llvm::outs() << "[Phi] Found " << Project.getCompilationUnits().size()
                 << " source files\n";
  }

  // Create output directory
  fs::create_directories(Project.getConfig().OutputDir);

  DiagnosticManager Diags;

  // Compile all units (lex + parse + register into modules)
  for (auto &Unit : Project.getCompilationUnits()) {
    if (Opts.Verbose) {
      llvm::outs() << "[Phi] Compiling: " << Unit->Filename << "\n";
    }

    // Add source to diagnostic manager
    Diags.getSrcManager().addSrcFile(Unit->Filename, Unit->Source);

    // Lex
    Unit->Tokens = Lexer(Unit->Source, Unit->Filename, &Diags).scan();

    if (Diags.hasError()) {
      return false;
    }

    // Parse
    auto PartialModule = Parser(Unit->Tokens, &Diags).parse();

    if (Diags.hasError()) {
      return false;
    }

    // Register into project (merges modules with same name)
    Project.registerIntoModule(std::move(PartialModule));
  }

  // Get all modules
  std::vector<ModuleDecl *> Modules;
  Modules.reserve(Project.getModules().size());

  for (auto &[Name, Mod] : Project.getModules()) {
    Modules.push_back(Mod.get());
  }

  auto Resolved = NameResolver(Modules, &Diags).resolve();
  auto Checked = TypeInferencer(Modules, &Diags).infer();
  for (auto &Mod : Checked) {
    Mod->emit(0);
  }

  if (Diags.hasError()) {
    return false;
  }

  // TODO: Linking (when codegen is done)
  // linkObjects(Project);

  std::println("Build successful (codegen not yet implemented)");
  return true;
}

//===----------------------------------------------------------------------===//
// Run Executable
//===----------------------------------------------------------------------===//

int PhiBuildSystem::run(const fs::path &Executable,
                        const std::vector<std::string> &Args) {
  if (!fs::exists(Executable)) {
    llvm::errs() << "Error: Executable not found: " << Executable << "\n";
    return 1;
  }

  std::string Cmd = Executable.string();
  for (const auto &Arg : Args) {
    Cmd += " " + Arg;
  }

  return std::system(Cmd.c_str());
}

//===----------------------------------------------------------------------===//
// Project Creation
//===----------------------------------------------------------------------===//

bool PhiBuildSystem::createProject(const std::string &Name) {
  fs::path ProjectDir = Name;

  if (fs::exists(ProjectDir)) {
    llvm::errs() << "Error: Directory '" << Name << "' already exists\n";
    return false;
  }

  // Create directory structure
  fs::create_directories(ProjectDir / "src");
  fs::create_directories(ProjectDir / "tests");

  // Create phi.toml
  std::ofstream TomlFile(ProjectDir / "phi.toml");
  TomlFile << "[package]\n";
  TomlFile << "name = \"" << Name << "\"\n";
  TomlFile << "version = \"0.1.0\"\n";
  TomlFile.close();

  // Create src/main.phi
  std::ofstream MainFile(ProjectDir / "src" / "main.phi");
  MainFile << "fun main() {\n";
  MainFile << "    \n";
  MainFile << "}\n";
  MainFile.close();

  // Create .gitignore
  std::ofstream GitignoreFile(ProjectDir / ".gitignore");
  GitignoreFile << ".phi/\n";
  GitignoreFile.close();

  std::println("Created project '{}'", Name);
  return true;
}

bool PhiBuildSystem::initProject() {
  fs::path CurrentDir = fs::current_path();

  if (fs::exists("phi.toml")) {
    llvm::errs() << "Error: phi.toml already exists in current directory\n";
    return false;
  }

  std::string ProjectName = CurrentDir.filename().string();

  // Create phi.toml
  std::ofstream TomlFile("phi.toml");
  TomlFile << "[package]\n";
  TomlFile << "name = \"" << ProjectName << "\"\n";
  TomlFile << "version = \"0.1.0\"\n";
  TomlFile.close();

  // Create .gitignore if it doesn't exist
  if (!fs::exists(".gitignore")) {
    std::ofstream GitignoreFile(".gitignore");
    GitignoreFile << ".phi/\n";
    GitignoreFile.close();
  }

  std::println("Initialized phi project in {}", CurrentDir.string());
  return true;
}

//===----------------------------------------------------------------------===//
// Clean
//===----------------------------------------------------------------------===//

void PhiBuildSystem::clean() {
  if (fs::exists(".phi") && fs::is_directory(".phi")) {
    fs::remove_all(".phi");
    std::println("Removed build directory .phi/");
  } else {
    std::println("Nothing to clean");
  }
}

//===----------------------------------------------------------------------===//
// Compilation Helpers
//===----------------------------------------------------------------------===//

bool PhiBuildSystem::compileFile(const fs::path &SourceFile,
                                 const fs::path &OutputFile, bool IsRelease,
                                 DiagnosticManager &Diags) {
  // Read source
  std::ifstream FileStream(SourceFile);
  if (!FileStream) {
    llvm::errs() << "Error: Cannot read file: " << SourceFile << "\n";
    return false;
  }

  std::string Source((std::istreambuf_iterator<char>(FileStream)),
                     std::istreambuf_iterator<char>());

  // Lex
  Diags.getSrcManager().addSrcFile(SourceFile.string(), Source);
  auto Tokens = Lexer(Source, SourceFile.string(), &Diags).scan();

  if (Diags.hasError()) {
    return false;
  }

  // Parse
  auto Module = Parser(Tokens, &Diags).parse();

  if (Diags.hasError()) {
    return false;
  }

  // Name resolution
  std::vector<ModuleDecl *> Modules = {Module.get()};
  auto Resolved = NameResolver(Modules, &Diags).resolve();

  if (Diags.hasError()) {
    return false;
  }

  // TODO: Type checking, code generation, linking
  // For now, just report success
  std::println("Compilation successful (codegen not yet implemented)");

  return true;
}

void PhiBuildSystem::compileUnit(CompilationUnit &Unit,
                                 DiagnosticManager &Diags) {
  // This is now handled inline in buildProject
  // Kept for compatibility if needed elsewhere
}

void PhiBuildSystem::linkObjects(PhiProject &Project) {
  std::vector<fs::path> ObjectFiles;

  for (auto &Unit : Project.getCompilationUnits()) {
    if (!Unit->ObjectFile.empty()) {
      ObjectFiles.push_back(Unit->ObjectFile);
    }
  }

  if (ObjectFiles.empty()) {
    return;
  }

  llvm::outs() << "[Phi] Linking " << ObjectFiles.size() << " object files\n";

  std::string Cmd = "clang++ ";
  for (auto &Obj : ObjectFiles) {
    Cmd += Obj.string() + " ";
  }

  fs::path OutputPath = Project.getConfig().OutputDir / "main";
  Cmd += "-o " + OutputPath.string();

  std::system(Cmd.c_str());
}

//===----------------------------------------------------------------------===//
// Project Config Helpers
//===----------------------------------------------------------------------===//

std::optional<fs::path> PhiBuildSystem::findPhiToml(const fs::path &StartDir) {
  fs::path Current = fs::absolute(StartDir);

  // Search upwards for phi.toml
  while (true) {
    fs::path Candidate = Current / "phi.toml";
    if (fs::exists(Candidate)) {
      return Candidate;
    }

    // Check if we've reached the root
    if (Current == Current.parent_path()) {
      break;
    }

    Current = Current.parent_path();
  }

  return std::nullopt;
}

std::string PhiBuildSystem::getProjectName(const fs::path &PhiTomlPath) {
  std::ifstream File(PhiTomlPath);
  std::string Line;

  while (std::getline(File, Line)) {
    if (Line.find("name = ") != std::string::npos) {
      // Extract name from: name = "project_name"
      auto Start = Line.find('"');
      auto End = Line.rfind('"');
      if (Start != std::string::npos && End != std::string::npos &&
          Start < End) {
        return Line.substr(Start + 1, End - Start - 1);
      }
    }
  }

  return "unknown";
}

} // namespace phi
