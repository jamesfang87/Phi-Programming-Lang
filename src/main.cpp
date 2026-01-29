#include "Driver/PhiBuildSystem.hpp"

#include <llvm/Support/raw_ostream.h>
#include <string>
#include <vector>

namespace phi {

void printUsage() {
  llvm::outs() << R"(Phi Programming Language Compiler

USAGE:
    phi <command> [options]

COMMANDS:
    compile <file>           Compile a single file
    build                    Build project
    run                      Build and run project
    new <name>               Create new project
    init                     Initialize Phi.toml in current directory
    clean                    Remove build artifacts
    --version, -v            Show version
    --help, -h               Show this help

COMPILE OPTIONS:
    -o <path>                Output path
    --release                Optimized build

BUILD/RUN OPTIONS:
    --release                Build in release mode
    --args <args...>         Arguments to pass to program (run only)

EXAMPLES:
    phi compile hello.phi
    phi compile file.phi -o output
    phi hello.phi            # Shorthand: compile and run
    phi new my_project
    phi build --release
    phi run --args input.txt --verbose
)";
}

void printVersion() {
  llvm::outs() << "Phi Programming Language Compiler version 0.1.0\n";
}

} // namespace phi

int main(int argc, char *argv[]) {
  using namespace phi;

  if (argc < 2) {
    printUsage();
    return 1;
  }

  std::string Command = argv[1];

  // Version and help
  if (Command == "--version" || Command == "-v") {
    printVersion();
    return 0;
  }

  if (Command == "--help" || Command == "-h") {
    printUsage();
    return 0;
  }

  // Compile command
  if (Command == "compile") {
    if (argc < 3) {
      llvm::errs() << "Error: Missing source file\n";
      llvm::errs() << "Usage: phi compile <file> [-o output] [--release]\n";
      return 1;
    }

    CompilerOptions Opts;
    Opts.Mode = BuildMode::SingleFile;
    Opts.InputFile = argv[2];

    for (int i = 3; i < argc; ++i) {
      std::string Arg = argv[i];

      if (Arg == "-o" && i + 1 < argc) {
        Opts.OutputPath = argv[++i];
      } else if (Arg == "--release") {
        Opts.IsRelease = true;
      } else if (Arg == "-v" || Arg == "--verbose") {
        Opts.Verbose = true;
      } else {
        llvm::errs() << "Error: Unknown option: " << Arg << "\n";
        return 1;
      }
    }

    return PhiBuildSystem::compileSingleFile(Opts.InputFile.value(), Opts) ? 0
                                                                           : 1;
  }

  // Build command
  if (Command == "build") {
    CompilerOptions Opts;
    Opts.Mode = BuildMode::Project;

    for (int i = 2; i < argc; ++i) {
      std::string Arg = argv[i];

      if (Arg == "--release") {
        Opts.IsRelease = true;
      } else if (Arg == "-v" || Arg == "--verbose") {
        Opts.Verbose = true;
      } else {
        llvm::errs() << "Error: Unknown option: " << Arg << "\n";
        return 1;
      }
    }

    return PhiBuildSystem::buildProject(Opts) ? 0 : 1;
  }

  // Run command
  if (Command == "run") {
    CompilerOptions Opts;
    Opts.Mode = BuildMode::Project;
    std::vector<std::string> RunArgs;
    bool CollectingArgs = false;

    for (int i = 2; i < argc; ++i) {
      std::string Arg = argv[i];

      if (Arg == "--args") {
        CollectingArgs = true;
      } else if (Arg == "--release") {
        Opts.IsRelease = true;
      } else if (Arg == "-v" || Arg == "--verbose") {
        Opts.Verbose = true;
      } else if (CollectingArgs) {
        RunArgs.push_back(Arg);
      } else {
        llvm::errs() << "Error: Unknown option: " << Arg << "\n";
        return 1;
      }
    }

    // Build first
    if (!PhiBuildSystem::buildProject(Opts)) {
      return 1;
    }

    // Find executable
    fs::path Exe =
        fs::path(".phi") / (Opts.IsRelease ? "release" : "debug") / "main";

    // Run
    return PhiBuildSystem::run(Exe, RunArgs);
  }

  // New command
  if (Command == "new") {
    if (argc < 3) {
      llvm::errs() << "Error: Missing project name\n";
      llvm::errs() << "Usage: phi new <name>\n";
      return 1;
    }

    return PhiBuildSystem::createProject(argv[2]) ? 0 : 1;
  }

  // Init command
  if (Command == "init") {
    return PhiBuildSystem::initProject() ? 0 : 1;
  }

  // Clean command
  if (Command == "clean") {
    PhiBuildSystem::clean();
    return 0;
  }

  // Shorthand: phi file.phi (compile and run)
  if (fs::path(Command).extension() == ".phi") {
    CompilerOptions Opts;
    Opts.Mode = BuildMode::SingleFile;
    Opts.InputFile = Command;

    // Create temp directory
    fs::path TempDir = ".phi/temp";
    fs::create_directories(TempDir);

    fs::path TempExe = TempDir / fs::path(Command).stem();
    Opts.OutputPath = TempExe;

    std::vector<std::string> RunArgs;
    bool CollectingArgs = false;

    for (int i = 2; i < argc; ++i) {
      std::string Arg = argv[i];

      if (Arg == "--args") {
        CollectingArgs = true;
      } else if (Arg == "--release") {
        Opts.IsRelease = true;
      } else if (CollectingArgs) {
        RunArgs.push_back(Arg);
      }
    }

    // Compile
    if (!PhiBuildSystem::compileSingleFile(Opts.InputFile.value(), Opts)) {
      return 1;
    }

    // Run
    return PhiBuildSystem::run(TempExe, RunArgs);
  }

  // Unknown command
  llvm::errs() << "Error: Unknown command: " << Command << "\n";
  llvm::errs() << "Run 'phi --help' for usage information\n";
  return 1;
}
