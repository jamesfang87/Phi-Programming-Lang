#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/TypeSystem/Type.hpp"
#include "SrcManager/SrcSpan.hpp"

namespace phi {

class TypeCtx {
public:
  TypeCtx();

  // factory methods
  static TypeRef getBuiltin(BuiltinTy::Kind, SrcSpan Span);
  static TypeRef getAdt(const std::string &Id, AdtDecl *D, SrcSpan Span);
  static TypeRef getTuple(const std::vector<TypeRef> &Elements, SrcSpan Span);
  static TypeRef getFun(const std::vector<TypeRef> &Params,
                        const TypeRef &Return, SrcSpan Span);
  static TypeRef getPtr(const TypeRef &Pointee, SrcSpan Span);
  static TypeRef getRef(const TypeRef &Pointee, SrcSpan Span);
  static TypeRef getVar(uint64_t N, VarTy::Domain Domain, SrcSpan Span);
  static TypeRef getVar(VarTy::Domain Domain, SrcSpan Span);
  static TypeRef getGeneric(const std::string &Id, TypeArgDecl *D,
                            SrcSpan Span);
  static TypeRef getApplied(TypeRef Base, std::vector<TypeRef> Args,
                            SrcSpan Span);
  static TypeRef getErr(SrcSpan Span);

  static std::vector<std::unique_ptr<Type>> &getAll();

private:
  // allocate new inst of type if not already present
  template <typename T, typename... Args> T *Allocate(Args &&...args) {
    auto UniPtr = std::make_unique<T>(std::forward<Args>(args)...);
    T *RawPtr = UniPtr.get();
    Arena.push_back(std::move(UniPtr));
    return RawPtr;
  }

  static TypeCtx &inst();

  BuiltinTy *builtin(BuiltinTy::Kind);
  AdtTy *adt(const std::string &Id, AdtDecl *D = nullptr);
  TupleTy *tuple(const std::vector<TypeRef> &Elements);
  FunTy *fun(const std::vector<TypeRef> &Params, const TypeRef &Ret);
  PtrTy *ptr(const TypeRef &Pointee);
  RefTy *ref(const TypeRef &Pointee);
  VarTy *var(uint64_t N, VarTy::Domain Domain);
  VarTy *var(VarTy::Domain Domain);
  GenericTy *generic(const std::string &Id, TypeArgDecl *D);
  AppliedTy *applied(TypeRef Base, std::vector<TypeRef> Args);
  ErrTy *err();

  std::vector<std::unique_ptr<Type>> Arena;

  // maps
  std::unordered_map<BuiltinTy::Kind, BuiltinTy *> Builtins;
  std::unordered_map<std::string, AdtTy *> Adts;
  std::unordered_map<TupleKey, TupleTy *, TupleKeyHash> Tuples;
  std::unordered_map<FunKey, FunTy *, FunKeyHash> Funs;
  std::unordered_map<AppliedKey, AppliedTy *, AppliedKeyHash> Applieds;
  std::unordered_map<const Type *, PtrTy *> Ptrs;
  std::unordered_map<const Type *, RefTy *> Refs;
  std::vector<VarTy *> Vars;
  std::vector<GenericTy *> Generics;
  ErrTy *Err;
};

} // namespace phi
