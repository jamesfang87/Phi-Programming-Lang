#include "Sema/TypeChecker.hpp"
#include <memory>
#include <vector>

using namespace phi;

std::pair<bool, std::vector<std::unique_ptr<Decl>>> TypeChecker::check() {
  bool Success = true;
  for (std::unique_ptr<Decl> &D : Ast) {
    Success = visit(*D) && Success;
  }

  return {Success, std::move(Ast)};
}
