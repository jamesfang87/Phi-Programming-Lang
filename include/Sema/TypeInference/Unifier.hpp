#pragma once

#include <llvm/ADT/DenseMap.h>

#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"

namespace phi {

class TypeUnifier {
  struct Node {
    Type *TheType;
    Type *Parent;
    uint32_t Size{1};
    std::optional<VarTy::Domain> Domain; // Only meaningful for Var roots
  };

  llvm::DenseMap<Type *, Node> Nodes;

public:
  TypeUnifier() {
    for (auto &T : TypeCtx::getAll()) {
      getNode(T.get());
    }
  }

  TypeRef resolve(TypeRef T) {
    getNode(T);

    auto *Resolved = find(T.getPtr());
    if (auto App = llvm::dyn_cast<AppliedTy>(Resolved)) {
      std::vector<TypeRef> InferredArgs;
      for (auto &Arg : App->getArgs()) {
        InferredArgs.push_back(resolve(Arg));
      }
      auto Res = TypeCtx::getApplied(resolve(App->getBase()), InferredArgs,
                                     T.getSpan());
      getNode(Res);
      return Res;
    }

    if (auto Fun = llvm::dyn_cast<FunTy>(Resolved)) {
      std::vector<TypeRef> InferredParams;
      for (auto &Arg : Fun->getParamTys()) {
        InferredParams.push_back(resolve(Arg));
      }
      auto Res = TypeCtx::getFun(InferredParams, resolve(Fun->getReturnTy()),
                                 T.getSpan());
      getNode(Res);
      return Res;
    }

    if (auto Tuple = llvm::dyn_cast<TupleTy>(Resolved)) {
      std::vector<TypeRef> InferredElems;
      for (auto &Arg : Tuple->getElementTys()) {
        InferredElems.push_back(resolve(Arg));
      }

      auto Res = TypeCtx::getTuple(InferredElems, T.getSpan());
      getNode(Res);
      return Res;
    }

    return TypeRef(find(T.getPtr()), T.getSpan());
  }

  bool unify(TypeRef A, TypeRef B);
  void emit() const;

private:
  Type *find(TypeRef T);
  Type *find(Type *T);

  Node &getNode(TypeRef T);
  Node &getNode(Type *T);

  bool unifyVars(TypeRef A, TypeRef B);
  bool unifyConcretes(TypeRef A, TypeRef B);
  bool unifyVarAndConcrete(TypeRef Var, TypeRef Con);
};

} // namespace phi
