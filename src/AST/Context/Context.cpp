#include "AST/TypeSystem/Context.hpp"

#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Type.hpp"

namespace phi {

TypeCtx::TypeCtx() {
  // Initialize builtins
  for (int k = static_cast<int>(BuiltinTy::i8);
       k <= static_cast<int>(BuiltinTy::Null); ++k) {
    auto K = static_cast<BuiltinTy::Kind>(k);
    auto *NewInst = Allocate<BuiltinTy>(K);
    Builtins.emplace(K, NewInst);
  }
  // Initialize error type
  Err = Allocate<ErrTy>();
}

TypeCtx &TypeCtx::inst() {
  static TypeCtx C;
  return C;
}

BuiltinTy *TypeCtx::builtin(BuiltinTy::Kind K) {
  auto It = Builtins.find(K);
  assert(It != Builtins.end() &&
         "All builtins are pre-allocated; they should all exist");
  return It->second;
}

AdtTy *TypeCtx::adt(const std::string &Id, AdtDecl *D) {
  auto It = Adts.find(Id);
  if (It != Adts.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<AdtTy>(Id, D);
  Adts[Id] = NewInst;
  return NewInst;
}

TupleTy *TypeCtx::tuple(const std::vector<TypeRef> &Elements) {
  TupleKey Key{Elements};
  auto It = Tuples.find(Key);
  if (It != Tuples.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<TupleTy>(Elements);
  Tuples.emplace(std::move(Key), NewInst);
  return NewInst;
}

FunTy *TypeCtx::fun(const std::vector<TypeRef> &Params, const TypeRef &Ret) {
  FunKey Key{.Params = Params, .Ret = Ret};
  auto It = Funs.find(Key);
  if (It != Funs.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<FunTy>(Params, Ret);
  Funs.emplace(std::move(Key), NewInst);
  return NewInst;
}

PtrTy *TypeCtx::ptr(const TypeRef &Pointee) {
  auto It = Ptrs.find(Pointee.getPtr());
  if (It != Ptrs.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<PtrTy>(Pointee);
  Ptrs[Pointee.getPtr()] = NewInst;
  return NewInst;
}

RefTy *TypeCtx::ref(const TypeRef &Pointee) {
  auto It = Refs.find(Pointee.getPtr());
  if (It != Refs.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<RefTy>(Pointee);
  Refs[Pointee.getPtr()] = NewInst;
  return NewInst;
}

VarTy *TypeCtx::var(uint64_t N, VarTy::Domain /*Constraints*/) {
  if (N >= Vars.size()) {
    return nullptr;
  }

  return Vars[N];
}

VarTy *TypeCtx::var(VarTy::Domain Domain) {
  auto *NewInst = Allocate<VarTy>(Vars.size(), Domain);
  Vars.push_back(NewInst);
  return NewInst;
}

GenericTy *TypeCtx::generic(const std::string &Id, TypeArgDecl *D) {
  auto *NewInst = Allocate<GenericTy>(Id, D);
  Generics.push_back(NewInst);
  return NewInst;
}

AppliedTy *TypeCtx::applied(TypeRef Base, std::vector<TypeRef> Args) {
  AppliedKey Key{Base, Args};
  auto It = Applieds.find(Key);
  if (It != Applieds.end()) {
    return It->second;
  }

  auto *NewInst = Allocate<AppliedTy>(Base, Args);
  Applieds.emplace(std::move(Key), NewInst);
  return NewInst;
}

ErrTy *TypeCtx::err() { return Err; }

TypeRef TypeCtx::getBuiltin(BuiltinTy::Kind K, SrcSpan Span) {
  auto *T = inst().builtin(K);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getAdt(const std::string &Id, AdtDecl *D, SrcSpan Span) {
  auto *T = inst().adt(Id, D);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getTuple(const std::vector<TypeRef> &Elements, SrcSpan Span) {
  auto *T = inst().tuple(Elements);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getFun(const std::vector<TypeRef> &Params, const TypeRef &Ret,
                        SrcSpan Span) {
  auto *T = inst().fun(Params, Ret);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getPtr(const TypeRef &Pointee, SrcSpan Span) {
  auto *T = inst().ptr(Pointee);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getRef(const TypeRef &Pointee, SrcSpan Span) {
  auto *T = inst().ref(Pointee);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getVar(uint64_t N, VarTy::Domain Domain, SrcSpan Span) {
  auto *T = inst().var(N, Domain);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getVar(VarTy::Domain Domain, SrcSpan Span) {
  auto *T = inst().var(Domain);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getGeneric(const std::string &Id, TypeArgDecl *D,
                            SrcSpan Span) {
  auto *T = inst().generic(Id, D);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getApplied(TypeRef Base, std::vector<TypeRef> Args,
                            SrcSpan Span) {
  auto *T = inst().applied(Base, Args);
  return {T, std::move(Span)};
}

TypeRef TypeCtx::getErr(SrcSpan Span) {
  auto *T = inst().err();
  return {T, std::move(Span)};
}

std::vector<std::unique_ptr<Type>> &TypeCtx::getAll() { return inst().Arena; }

} // namespace phi
