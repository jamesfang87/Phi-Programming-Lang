#include "AST/TypeSystem/Type.hpp"

#include <llvm-18/llvm/ADT/DenseSet.h>
#include <llvm-18/llvm/ADT/StringExtras.h>
#include <llvm-18/llvm/ADT/TypeSwitch.h>
#include <llvm-18/llvm/Support/Casting.h>

#include <algorithm>
#include <cstdint>
#include <format>
#include <iterator>
#include <ranges>
#include <string>

#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeInference/Substitution.hpp"

namespace phi {

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
  case Kind::Error:
    return "!";
  }
}

std::string AdtTy::toString() const { return getId(); }

std::string TupleTy::toString() const {
  std::string Ret = "(";
  for (uint64_t i = 0; i < ElementTys.size(); i++) {
    Ret += ElementTys[i].toString();
    if (i < ElementTys.size() - 1)
      Ret += ", ";
    else
      Ret += ")";
  }
  return Ret;
}

std::string FunTy::toString() const {
  std::string Params = "(";
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

std::string ErrTy::toString() const { return "Error"; }

bool VarTy::occursIn(TypeRef Other) const {
  return llvm::TypeSwitch<const Type *, bool>(Other.getPtr())
      .Case<VarTy>([&](auto *Var) { return Var->getN() == getN(); })
      .Case<TupleTy>([&](auto *Tuple) {
        for (const auto Element : Tuple->getElementTys()) {
          if (occursIn(Element))
            return true;
        }
        return false;
      })
      .Case<FunTy>([&](auto *Fun) {
        if (occursIn(Fun->getReturnTy())) {
          return true;
        }

        for (const auto Param : Fun->getParamTys()) {
          if (occursIn(Param))
            return true;
        }
        return false;
      })
      .Case<PtrTy>([&](auto *Ptr) { return occursIn(Ptr->getPointee()); })
      .Case<RefTy>([&](auto *Ref) { return occursIn(Ref->getPointee()); })
      .Default([](const Type * /*T*/) {
        return false; // ErrTy, AdtTy, BuiltinTy
      });
}

std::expected<Substitution, Diagnostic> VarTy::bind(TypeRef T) {
  if (this == T.getPtr()) {
    const auto *VT = llvm::dyn_cast<VarTy>(T.getPtr());
    assert(VT);
    assert(VT->getN() == getN());

    return Substitution();
  }

  // occurs check
  if (occursIn(T))
    return std::unexpected(
        error("BindVar failed due to failed occursIn check").build());

  // make sure constraints are compatible
  if (auto *Other = llvm::dyn_cast<VarTy>(T.getPtr())) {
    const auto &A = this->Constraints;
    const auto &B = Other->getConstraints();

    // Build membership set for pointer identity
    llvm::DenseSet<const Type *> Allowed;
    Allowed.reserve(A.size());
    for (const auto &T : A)
      Allowed.insert(T.getPtr());

    auto Shared = B | std::views::filter([&](const TypeRef &T) {
                    return Allowed.contains(T.getPtr());
                  });

    if (std::ranges::empty(Shared)) {
      // Convert TypeRef vectors to string vectors
      llvm::SmallVector<std::string, 8> LeftStrings, RightStrings;
      LeftStrings.reserve(A.size());
      RightStrings.reserve(B.size());

      for (const auto &T : A)
        LeftStrings.push_back(T.toString());
      for (const auto &T : B)
        RightStrings.push_back(T.toString());

      auto D =
          error("incompatible type constraints between type variables")
              .with_note(
                  std::format("left allows: {}", llvm::join(LeftStrings, ", ")))
              .with_note(std::format("right allows: {}",
                                     llvm::join(RightStrings, ", ")))
              .with_note(
                  "consider adding an explicit type annotation to disambiguate")
              .build();

      return std::unexpected(std::move(D));
    }
  }

  // Substitution of Var -> T
  Substitution S(this, T);
  return S;
};

} // namespace phi
