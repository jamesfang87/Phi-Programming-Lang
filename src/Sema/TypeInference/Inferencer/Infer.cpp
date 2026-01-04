#include "Sema/TypeInference/Inferencer.hpp"

#include <string>

#include <llvm/Support/Casting.h>

#include "AST/TypeSystem/Type.hpp"
#include "Sema/TypeInference/Unifier.hpp"

namespace phi {

std::vector<std::unique_ptr<Decl>> TypeInferencer::infer() {
  for (auto &Decl : Ast)
    visit(*Decl);

  for (auto &Decl : Ast)
    finalize(*Decl);

  Unifier.emit();

  return std::move(Ast);
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
