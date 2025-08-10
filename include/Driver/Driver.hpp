#include "Diagnostics/DiagnosticManager.hpp"
#include <string>

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
