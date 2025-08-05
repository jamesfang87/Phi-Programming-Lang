#include "Driver/Driver.hpp"

#include "CodeGen/CodeGen.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/Sema.hpp"
#include <cstdlib>
#include <format>

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

  // Code Generation
  phi::CodeGen codegen(std::move(resolved_ast), path);
  codegen.generate();

  // Output IR to file
  std::string ir_filename = path;
  size_t dot_pos = ir_filename.find_last_of('.');
  if (dot_pos != std::string::npos) {
    ir_filename = ir_filename.substr(0, dot_pos);
  }
  ir_filename += ".ll";

  codegen.outputIR(ir_filename);

  system(std::format("clang {} -o output", ir_filename).c_str());
  return true;
}

} // namespace phi
