#include "Diagnostics/DiagnosticManager.hpp"
#include <string>

namespace phi {

class PhiCompiler {
public:
    PhiCompiler(std::string src,
                std::string path,
                std::shared_ptr<DiagnosticManager> diagnostic_manager);
    ~PhiCompiler();

    bool compile();

private:
    std::string src_file;
    std::string path;
    std::shared_ptr<DiagnosticManager> diagnostic_manager;
};

} // namespace phi
