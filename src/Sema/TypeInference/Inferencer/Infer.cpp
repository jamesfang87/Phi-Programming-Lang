#include "Sema/TypeInference/Inferencer.hpp"

#include <string>

#include <llvm/Support/Casting.h>

#include "AST/TypeSystem/Type.hpp"
#include "Sema/TypeInference/Unifier.hpp"

namespace phi {

std::vector<ModuleDecl *> TypeInferencer::infer() {
  for (auto &Mod : Modules)
    visit(*Mod);

  for (auto &Mod : Modules)
    finalize(*Mod);

  return std::move(Modules);
}

std::string TypeInferencer::toString(TypeRef T) {
  auto A = Unifier.resolve(T);
  if (A.isVar()) {
    auto Var = llvm::dyn_cast<VarTy>(A.getPtr());
    switch (Var->getDomain()) {
    case VarTy::Any:
      return "[Any]";
    case VarTy::Int:
      return "[Int]";
    case VarTy::Float:
      return "[Float]";
    case VarTy::Adt:
      return "[ADT]";
    }
  }

  return A.toString();
}

} // namespace phi
