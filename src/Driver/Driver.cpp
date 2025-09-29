#include "Driver/Driver.hpp"

#include <cstdlib>
#include <string>

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

  auto [NameResolutionSuccess, ResolvedNames] =
      NameResolver(std::move(Ast), DiagnosticMan).resolveNames();
  auto InferredTypes = TypeInferencer(std::move(ResolvedNames)).inferProgram();
  auto [TypeCheckingSuccess, CheckedTypes] =
      TypeChecker(std::move(InferredTypes), DiagnosticMan).check();
  if (DiagnosticMan->error_count() > 0) {
    return false;
  }

  if (DiagnosticMan->error_count() > 0) {
    return false;
  }

  // Code Generation
  phi::CodeGen CodeGen(std::move(CheckedTypes), Path);
  CodeGen.generate();

  return true;
}

} // namespace phi
