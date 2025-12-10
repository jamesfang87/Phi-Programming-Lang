#include "Driver/Driver.hpp"

#include <cstdlib>
#include <memory>
#include <optional>
#include <print>
#include <string>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/Lexer.hpp"
#include "Parser/Parser.hpp"
#include "Sema/NameResolver.hpp"
#include "Sema/TypeChecker.hpp"
#include "Sema/TypeInference/Infer.hpp"

namespace phi {

PhiCompiler::PhiCompiler(std::string Src, std::string Path,
                         std::shared_ptr<DiagnosticManager> DiagMan)
    : SrcFile(std::move(Src)), Path(std::move(Path)),
      DiagnosticMan(std::move(DiagMan)) {}

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

  for (const auto &D : ResolvedNames) {
    D->emit(0);
  }

  auto InferredTypes =
      TypeInferencer(std::move(ResolvedNames), DiagnosticMan).inferProgram();

  for (const auto &D : InferredTypes) {
    D->emit(0);
  }

  auto [TypeCheckingSuccess, CheckedTypes] =
      TypeChecker(std::move(InferredTypes), DiagnosticMan).check();
  if (DiagnosticMan->error_count() > 0) {
    return false;
  }

  // Code Generation
  phi::CodeGen CodeGen(std::move(CheckedTypes), Path);
  CodeGen.generate();

  return true;
}

std::optional<std::vector<std::unique_ptr<Decl>>> PhiCompiler::compileToAST() {
  auto Tokens = Lexer(SrcFile, Path, DiagnosticMan).scan();
  if (DiagnosticMan->error_count() > 0) {
    return std::nullopt;
  }

  auto Ast = Parser(SrcFile, Path, Tokens, DiagnosticMan).parse();
  if (DiagnosticMan->error_count() > 0) {
    return std::nullopt;
  }

  auto [NameResolutionSuccess, ResolvedNames] =
      NameResolver(std::move(Ast), DiagnosticMan).resolveNames();
  auto InferredTypes =
      TypeInferencer(std::move(ResolvedNames), DiagnosticMan).inferProgram();
  auto [TypeCheckingSuccess, CheckedTypes] =
      TypeChecker(std::move(InferredTypes), DiagnosticMan).check();
  if (DiagnosticMan->error_count() > 0) {
    return std::nullopt;
  }

  return std::move(CheckedTypes);
}

} // namespace phi
