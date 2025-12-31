#include "Sema/TypeInference/Substitution.hpp"

#include <ranges>

#include "llvm/ADT/STLExtras.h"
#include "llvm/ADT/TypeSwitch.h"
#include "llvm/ADT/iterator_range.h"

#include "AST/TypeSystem/Context.hpp"

namespace phi {

[[nodiscard]] TypeRef Substitution::apply(const TypeRef &T) const {
  return llvm::TypeSwitch<Type *, TypeRef>(T.getPtr())

      // ---------- Type variable ----------
      .Case<VarTy>([&](VarTy *V) -> TypeRef {
        auto It = Map.find(V);
        if (It != Map.end())
          return apply(It->second);
        return T;
      })

      // ---------- Type constructors ----------
      .Case<BuiltinTy>([&](const BuiltinTy * /*B*/) -> TypeRef { return T; })
      .Case<AdtTy>([&](const AdtTy * /*A*/) -> TypeRef { return T; })

      // ---------- Function ----------
      .Case<FunTy>([&](const FunTy *F) -> TypeRef {
        auto R = llvm::map_range(F->getParamTys(), [&](const TypeRef &Param) {
          return apply(Param);
        });
        std::vector<TypeRef> Params(R.begin(), R.end());
        auto Ret = apply(F->getReturnTy());

        return TypeCtx::getFun(Params, Ret, T.getSpan());
      })

      // ---------- Tuple (type application) ----------
      .Case<TupleTy>([&](const TupleTy *Tup) -> TypeRef {
        auto R = llvm::map_range(Tup->getElementTys(),
                                 [&](const TypeRef &E) { return apply(E); });
        std::vector<TypeRef> Elems(R.begin(), R.end());

        return TypeCtx::getTuple(Elems, T.getSpan());
      })

      // ---------- Pointer (type application) ----------
      .Case<PtrTy>([&](const PtrTy *P) -> TypeRef {
        return TypeCtx::getPtr(apply(P->getPointee()), T.getSpan());
      })

      // ---------- Reference (type application) ----------
      .Case<RefTy>([&](const RefTy *R) -> TypeRef {
        return TypeCtx::getRef(apply(R->getPointee()), T.getSpan());
      })

      // ---------- Error ----------
      .Case<ErrTy>([&](const ErrTy *) -> TypeRef { return T; })

      // ---------- Exhaustiveness ----------
      .Default([&](Type *) -> TypeRef {
        llvm_unreachable("unknown Type kind in Substitution::apply");
      });
}

void Substitution::compose(const Substitution &Other) {
  if (Other.empty()) {
    return;
  }

  for (auto &T : Map | std::views::values)
    T = Other.apply(T);

  for (const auto &[TypeVar, TypeRef] : Other.Map)
    Map.insert_or_assign(TypeVar, TypeRef);
}

} // namespace phi
