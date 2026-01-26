#include "Driver/PhiBuildSystem.hpp"

#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include <filesystem>

namespace phi {

CompilerOptions PhiBuildSystem::parseCommandLine(int argc, char **argv,
                                                 fs::path &ProjectPath) {
  CompilerOptions Options;
  ProjectPath.clear();

  for (int i = 1; i < argc; ++i) {
    std::string Arg = argv[i];
    if (Arg == "--project" && i + 1 < argc) {
      ProjectPath = argv[++i];
    } else if (Arg == "-v") {
      Options.Verbose = true;
    } else if (Arg == "-S") {
      Options.EmitAssembly = true;
    } else if (Arg == "-emit-llvm") {
      Options.EmitLLVM = true;
    } else if (Arg.starts_with("-O")) {
      Options.OptimizationLevel = std::stoi(Arg.substr(2));
    } else if (Arg == "-c") {
      Options.CompileOnly = true;
    } else if (Arg == "--dump-tokens") {
      Options.DumpTokens = true;
    } else if (Arg == "--dump-ast") {
      Options.DumpAST = true;
    }
  }

  // Auto-discover project if not specified
  if (ProjectPath.empty()) {
    auto Discovered = findProjectFile(fs::current_path());
    if (Discovered.has_value()) {
      ProjectPath = Discovered.value();
      if (Options.Verbose)
        llvm::outs() << "[Phi] Auto-discovered project: " << ProjectPath
                     << "\n";
    } else {
      llvm::errs() << "No project file found (phi.json)!\n";
      std::exit(1);
    }
  }

  return Options;
}

std::optional<fs::path>
PhiBuildSystem::findProjectFile(const fs::path &StartDir) {
  fs::path Candidate = fs::absolute(StartDir) / "phi.json";

  if (fs::exists(Candidate)) {
    return Candidate;
  }

  return std::nullopt;
}

void PhiBuildSystem::compileUnit(CompilationUnit &Unit) {
  if (Options.Verbose)
    llvm::outs() << "[Phi] Compiling: " << Unit.Filename << "\n";

  Diags.getSrcManager().addSrcFile(Unit.Filename, Unit.Source);
  Unit.Tokens = Lexer(Unit.Source, Unit.Filename, &Diags).scan();
  Project.registerIntoModule(Parser(Unit.Tokens, &Diags).parse());
}

void PhiBuildSystem::linkObjects() {
  std::vector<fs::path> ObjectFiles;
  for (auto &Unit : Project.getCompilationUnits()) {
    if (!Unit->ObjectFile.empty())
      ObjectFiles.push_back(Unit->ObjectFile);
  }

  if (Options.Verbose) {
    llvm::outs() << "[Phi] Linking " << ObjectFiles.size() << " object files\n";
  }

  std::string Cmd = "clang++ ";
  for (auto &Obj : ObjectFiles)
    Cmd += Obj.string() + " ";
  Cmd += "-o " + (Project.getConfig().OutputDir / "phi.out").string();
  std::system(Cmd.c_str());
}

int PhiBuildSystem::build() {
  for (auto &Unit : Project.getCompilationUnits()) {
    compileUnit(*Unit);
  }

  std::vector<ModuleDecl *> Modules;
  Modules.reserve(Project.getModules().size());
  for (auto &[Name, Mod] : Project.getModules()) {
    Mod->emit(0);
    Modules.push_back(Mod.get());
  }
  auto Res = NameResolver(Modules, &Diags).resolve();

  if (!Options.EmitLLVM && !Options.EmitAssembly && !Options.CompileOnly)
    linkObjects();

  return Diags.hasError() ? 1 : 0;
}

} // namespace phi
