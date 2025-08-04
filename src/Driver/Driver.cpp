#include "Driver/Driver.hpp"

#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"

namespace phi {

PhiCompiler::PhiCompiler(std::string src, std::string path,
                         std::shared_ptr<DiagnosticManager> diagnostic_manager)
    : srcFile(std::move(src)), path(std::move(path)),
      diagnosticManager(std::move(diagnostic_manager)) {}

PhiCompiler::~PhiCompiler() = default;

bool PhiCompiler::compile() {
  auto [tokens, scan_success] = Lexer(srcFile, path, diagnosticManager).scan();
  if (!scan_success) {
    return false;
  }

  auto [ast, parse_success] =
      Parser(srcFile, path, tokens, diagnosticManager).parse();
  if (!parse_success) {
    return false;
  }

  auto [success, resolved_ast] = Sema(std::move(ast)).resolveAST();
  if (!success) {
    return false;
  }

  // Implementation of the compile method
  return true;
}

} // namespace phi
