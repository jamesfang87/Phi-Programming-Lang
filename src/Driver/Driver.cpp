#include "Driver/Driver.hpp"

#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"

namespace phi {

PhiCompiler::PhiCompiler(std::string src,
                         std::string path,
                         std::shared_ptr<DiagnosticManager> diagnostic_manager)
    : src_file(std::move(src)),
      path(std::move(path)),
      diagnostic_manager(std::move(diagnostic_manager)) {}

PhiCompiler::~PhiCompiler() = default;

bool PhiCompiler::compile() {
    auto [tokens, scan_success] = Lexer(src_file, path, diagnostic_manager).scan();
    if (!scan_success) {
        return false;
    }

    auto [ast, parse_success] = Parser(src_file, path, tokens, diagnostic_manager).parse();
    if (!parse_success) {
        return false;
    }

    auto [success, resolved_ast] = Sema(std::move(ast)).resolve_ast();
    if (!success) {
        return false;
    }

    // Implementation of the compile method
    return true;
}

} // namespace phi
