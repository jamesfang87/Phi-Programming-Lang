#include "Diagnostics/DiagnosticManager.hpp"
#include <string>

namespace phi {

class PhiCompiler {
public:
  PhiCompiler(std::string src, std::string path,
              std::shared_ptr<DiagnosticManager> diagnosticsManager);
  ~PhiCompiler();

  bool compile();

private:
  std::string srcFile;
  std::string path;
  std::shared_ptr<DiagnosticManager> diagnosticManager;
};

} // namespace phi
