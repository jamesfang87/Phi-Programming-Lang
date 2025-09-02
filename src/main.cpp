#include <fstream>
#include <iostream>
#include <string>

#include "Driver/Driver.hpp"

std::string readFileToStr(const std::string &Path) {
  std::ifstream Fin(Path);
  if (!Fin.is_open()) {
    throw std::runtime_error("Could not open file: " + Path);
  }
  return std::string{std::istreambuf_iterator(Fin),
                     std::istreambuf_iterator<char>()};
}

int main(int argc, char *argv[]) {
  if (argc < 2) {
    std::cerr << "Usage: " << argv[0] << " <filename>\n";
    return 1;
  }

  std::string Path;

  // Parse command line arguments
  for (int i = 1; i < argc; i++) {
    std::string Arg = argv[i];
    if (Path.empty()) {
      Path = Arg;
    } else {
      std::cerr << "Unknown argument: " << Arg << "\n";
      std::cerr << "Usage: " << argv[0] << " <filename>";
      return 1;
    }
  }

  if (Path.empty()) {
    std::cerr << "Error: No filename provided\n";
    std::cerr << "Usage: " << argv[0] << " <filename>\n";
    return 1;
  }

  std::string Src = readFileToStr(Path);
  auto SrcMan = std::make_shared<phi::SrcManager>();
  auto DiagMan = std::make_shared<phi::DiagnosticManager>(SrcMan);

  // Register source content for diagnostic display
  SrcMan->addSrcFile(Path, Src);

  phi::PhiCompiler Driver(Src, Path, DiagMan);
  Driver.compile();
  return 0;
}
