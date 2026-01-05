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

  TypeRef resolve(TypeRef T) { return TypeRef(find(T.getPtr()), T.getSpan()); }
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
