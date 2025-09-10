#include "Driver/Driver.hpp"

#include <cstdlib>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"

namespace phi {

PhiCompiler::PhiCompiler(std::string src, std::string path,
                         std::shared_ptr<DiagnosticManager> diagnosticsMan)
    : SrcFile(std::move(src)), Path(std::move(path)),
      DiagnosticMan(std::move(diagnosticsMan)) {}

PhiCompiler::~PhiCompiler() = default;

bool PhiCompiler::compile() {
  auto Tokens = Lexer(SrcFile, Path, DiagnosticMan).scan();
  if (DiagnosticMan->error_count() > 0) {
    return false;
  }

  auto Ast = Parser(SrcFile, Path, Tokens, DiagnosticMan).parse();
  if (DiagnosticMan->error_count() > 0) {
    return false;
  }

  auto [Success, ResolvedNames] =
      NameResolver(std::move(Ast), DiagnosticMan).resolveNames();
  auto ResolvedTypes = TypeInferencer(std::move(ResolvedNames)).inferProgram();
  auto [S, A] = TypeChecker(std::move(ResolvedTypes)).check();

  for (auto &D : A) {
    D->emit(0);
  }

  // Code Generation
  phi::CodeGen codegen(std::move(A), Path);
  codegen.generate();

  // Output IR to file
  std::string ir_filename = Path;
  size_t dot_pos = ir_filename.find_last_of('.');
  if (dot_pos != std::string::npos) {
    ir_filename = ir_filename.substr(0, dot_pos);
  }
  ir_filename += ".ll";

  codegen.outputIR(ir_filename);

  system(std::format("clang {}", ir_filename).c_str());
  return true;
}

} // namespace phi
