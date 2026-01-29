#include "AST/TypeSystem/Type.hpp"

#include <llvm/ADT/DenseSet.h>
#include <llvm/ADT/StringExtras.h>
#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include <cstdint>
#include <format>
#include <string>

namespace phi {

Type *Type::getUnderlying() {
  Type *Current = this;

  while (true) {
    if (auto *Ptr = llvm::dyn_cast<PtrTy>(Current)) {
      Current = Ptr->getPointee().getPtr();
    } else if (auto *Ref = llvm::dyn_cast<RefTy>(Current)) {
      Current = Ref->getPointee().getPtr();
    } else if (auto *Applied = llvm::dyn_cast<AppliedTy>(Current)) {
      Current = Applied->getBase().getPtr();
    } else {
      break;
    }
  }

  return Current;
}

TypeRef TypeRef::getUnderlying() { return TypeRef(Ptr->getUnderlying(), Span); }

std::string BuiltinTy::toString() const {
  switch (getBuiltinKind()) {
  case Kind::i8:
    return "i8";
  case Kind::i16:
    return "i16";
  case Kind::i32:
    return "i32";
  case Kind::i64:
    return "i64";
  case Kind::u8:
    return "u8";
  case Kind::u16:
    return "u16";
  case Kind::u32:
    return "u32";
  case Kind::u64:
    return "u64";
  case Kind::f32:
    return "f32";
  case Kind::f64:
    return "f64";
  case Kind::String:
    return "string";
  case Kind::Char:
    return "char";
  case Kind::Bool:
    return "bool";
  case Kind::Range:
    return "range";
  case Kind::Null:
    return "null";
  }
}

std::string AdtTy::toString() const { return getId(); }

std::string AppliedTy::toString() const {
  std::string Result = Base.toString() + "<";
  for (size_t i = 0; i < Args.size(); ++i) {
    Result += Args[i].toString();
    if (i < Args.size() - 1)
      Result += ", ";
  }
  Result += ">";
  return Result;
}

std::string TupleTy::toString() const {
  std::string Elems = "(";
  if (Elems.empty()) {
    Elems = "()";
  }
  for (uint64_t i = 0; i < ElementTys.size(); i++) {
    Elems += ElementTys[i].toString();
    if (i < ElementTys.size() - 1)
      Elems += ", ";
    else
      Elems += ")";
  }
  return Elems;
}

std::string FunTy::toString() const {
  std::string Params = "(";
  if (ParamTys.empty()) {
    Params = "()";
  }
  for (uint64_t i = 0; i < ParamTys.size(); i++) {
    Params += ParamTys[i].toString();
    if (i < ParamTys.size() - 1)
      Params += ", ";
    else
      Params += ")";
  }
  return std::format("fun{} -> {}", Params, ReturnTy.toString());
}

std::string PtrTy::toString() const { return "*" + Pointee.toString(); }

std::string RefTy::toString() const { return "&" + Pointee.toString(); }

std::string VarTy::toString() const { return "T" + std::to_string(N); }

std::string GenericTy::toString() const { return "Generic: " + Id; };

std::string ErrTy::toString() const { return "Error"; }

bool VarTy::occursIn(TypeRef Other) const {
  return llvm::TypeSwitch<const Type *, bool>(Other.getPtr())
      .Case<VarTy>([&](auto *Var) { return Var->getN() == getN(); })
      .Case<TupleTy>([&](auto *Tuple) {
        for (const auto &Element : Tuple->getElementTys()) {
          if (occursIn(Element))
            return true;
        }
        return false;
      })
      .Case<FunTy>([&](auto *Fun) {
        if (occursIn(Fun->getReturnTy())) {
          return true;
        }

        for (const auto &Param : Fun->getParamTys()) {
          if (occursIn(Param))
            return true;
        }
        return false;
      })
      .Case<PtrTy>([&](auto *Ptr) { return occursIn(Ptr->getPointee()); })
      .Case<RefTy>([&](auto *Ref) { return occursIn(Ref->getPointee()); })
      .Case<AppliedTy>([&](auto *Applied) {
        if (occursIn(Applied->getBase()))
          return true;
        for (const auto &Arg : Applied->getArgs()) {
          if (occursIn(Arg))
            return true;
        }
        return false;
      })
      .Default([](const Type * /*T*/) {
        return false; // ErrTy, AdtTy, BuiltinTy
      });
}

bool VarTy::accepts(TypeRef T) const {
  if (T.isVar()) {
    return unifyDomain(llvm::dyn_cast<VarTy>(T.getPtr())) != std::nullopt;
  }

  if (!T.isBuiltin()) {
    return !occursIn(T);
  }

  auto *B = llvm::dyn_cast<BuiltinTy>(T.getPtr());
  using K = BuiltinTy::Kind;

  switch (TheDomain) {
  case Domain::Any:
    return true;
  case Domain::Int:
    return llvm::is_contained(
        {K::i8, K::i16, K::i32, K::i64, K::u8, K::u16, K::u32, K::u64},
        B->getBuiltinKind());
  case Domain::Float:
    return llvm::is_contained({K::f32, K::f64}, B->getBuiltinKind());
  case Domain::Adt:
    return T.isAdt();
  }
  std::unreachable();
}

std::optional<VarTy::Domain> VarTy::unifyDomain(const VarTy *Var) const {
  if (this->TheDomain == Domain::Any)
    return Var->getDomain();
  if (Var->getDomain() == Domain::Any)
    return this->TheDomain;
  if (this->TheDomain == Var->getDomain())
    return this->TheDomain;
  return std::nullopt;
}

} // namespace phi
