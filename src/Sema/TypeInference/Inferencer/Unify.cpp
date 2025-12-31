#include "Sema/TypeInference/Inferencer.hpp"

#include <expected>

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeInference/Substitution.hpp"

namespace phi {
std::expected<Substitution, Diagnostic> TypeInferencer::unify(TypeRef A,
                                                              TypeRef B) {

  using SubResult = std::expected<Substitution, Diagnostic>;

  // 1. Identity
  if (A.getPtr() == B.getPtr()) {
    return Substitution();
  }

  // 2. Err can be coerced into any type
  if (A.isErr() || B.isErr())
    return Substitution();

  // 3. Var
  if (auto Var = llvm::dyn_cast<VarTy>(A.getPtr())) {
    return Var->bind(B);
  }

  if (auto Var = llvm::dyn_cast<VarTy>(B.getPtr())) {
    return Var->bind(A);
  }

  // A and B must be the same type now
  if (A.getPtr()->getKind() != B.getPtr()->getKind()) {
    return std::unexpected(error("A and B must be same kind").build());
  }

  return llvm::TypeSwitch<const Type *, SubResult>(A.getPtr())
      .Case<AdtTy>([&](const AdtTy *Adt) -> SubResult {
        auto Other = llvm::dyn_cast<AdtTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        if (Adt->getId() != Other->getId()) {
          return std::unexpected(error("Bad").build());
        }
        return Substitution();
      })
      .Case<TupleTy>([&](const TupleTy *Tuple) -> SubResult {
        auto Other = llvm::dyn_cast<TupleTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        const auto &ElemsA = Tuple->getElementTys();
        const auto &ElemsB = Other->getElementTys();

        if (ElemsA.size() != ElemsB.size())
          return std::unexpected(error("Tuple arity mismatch").build());

        Substitution S; // accumulated substitution

        for (auto [Aty, Bty] : llvm::zip(ElemsA, ElemsB)) {
          // Apply accumulated substitution before recursive unify
          TypeRef Left = S.apply(Aty);
          TypeRef Right = S.apply(Bty);

          auto Res = unify(Left, Right);
          if (!Res)
            return Res; // propagate diagnostic

          S.compose(*Res);
        }

        return S;
      })
      .Case<FunTy>([&](const FunTy *Fun) -> SubResult {
        auto Other = llvm::dyn_cast<FunTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        auto Res = unify(Fun->getReturnTy(), Other->getReturnTy());
        if (!Res) {
          return Res;
        }

        const auto &ParamsA = Fun->getParamTys();
        const auto &ParamsB = Other->getParamTys();

        if (ParamsA.size() != ParamsB.size())
          return std::unexpected(error("Param arity mismatch").build());

        Substitution S; // accumulated substitution

        for (auto [Aty, Bty] : llvm::zip(ParamsA, ParamsB)) {
          // Apply accumulated substitution before recursive unify
          TypeRef Left = S.apply(Aty);
          TypeRef Right = S.apply(Bty);

          auto Res = unify(Left, Right);
          if (!Res)
            return Res; // propagate diagnostic

          S.compose(*Res);
        }

        return S;
      })
      .Case<PtrTy>([&](const PtrTy *Ptr) {
        auto Other = llvm::dyn_cast<PtrTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        return unify(Ptr->getPointee(), Other->getPointee());
      })
      .Case<RefTy>([&](const RefTy *Ref) {
        auto Other = llvm::dyn_cast<RefTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        return unify(Ref->getPointee(), Other->getPointee());
      })
      .Case<BuiltinTy>([&](const BuiltinTy *Builtin) -> SubResult {
        auto Other = llvm::dyn_cast<BuiltinTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        if (Builtin->getBuiltinKind() != Other->getBuiltinKind()) {
          return std::unexpected(error("Bad").build());
        }
        return Substitution();
      })
      .Default([](const Type * /*T*/) {
        std::unreachable();
        return std::unexpected(
            error("Reached std::unreachable in unification").build());
      });
}

} // namespace phi
