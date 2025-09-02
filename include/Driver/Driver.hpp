#pragma once

#include <string>

#include "Diagnostics/DiagnosticManager.hpp"

namespace phi {

class PhiCompiler {
public:
  PhiCompiler(std::string Src, std::string Path,
              std::shared_ptr<DiagnosticManager> DiagnosticsMan);
  ~PhiCompiler();

  bool compile();

private:
  std::string SrcFile;
  std::string Path;
  std::shared_ptr<DiagnosticManager> DiagnosticMan;
};

} // namespace phi
