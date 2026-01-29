#include "Sema/NameResolution/NameResolver.hpp"

#include "AST/TypeSystem/Type.hpp"

#include <llvm/ADT/TypeSwitch.h>

#include <iostream>

namespace phi {

bool NameResolver::visit(TypeRef T) {

  if (!T.getPtr())
    return false;

  return llvm::TypeSwitch<const Type *, bool>(T.getPtr())
      .Case<AdtTy>([&](auto *Adt) {
        auto *Decl = SymbolTab.lookup(Adt->getId());
        if (!Decl) {
          emitTypeNotFound(Adt->getId(), T.getSpan().Start);
          return false;
        }
        Adt->setDecl(Decl);
        return true;
      })
      .Case<AppliedTy>([&](auto *App) {
        bool Success = visit(App->getBase());
        for (const auto &Arg : App->getArgs()) {
          Success = visit(Arg) && Success;
        }
        return Success;
      })
      .Case<GenericTy>([&](auto *Generic) {
        // this branch should only run...
        if (Generic->getDecl()) {
          return true;
        }

        std::cerr << "Generic type fall back to lookup";
        auto *Decl = SymbolTab.lookupTypeArg(Generic->getId());
        if (!Decl) {
          emitTypeNotFound(Generic->getId(), T.getSpan().Start);
          return false;
        }
        Generic->setDecl(Decl);
        return true;
      })
      .Case<TupleTy>([&](auto *Tuple) {
        bool Success = true;
        for (const auto &Element : Tuple->getElementTys()) {
          Success = visit(Element) && Success;
        }
        return Success;
      })
      .Case<FunTy>([&](auto *Fun) {
        bool Success = visit(Fun->getReturnTy());
        for (const auto &Param : Fun->getParamTys()) {
          Success = visit(Param) && Success;
        }
        return Success;
      })
      .Case<PtrTy>([&](auto *Ptr) { return visit(Ptr->getPointee()); })
      .Case<RefTy>([&](auto *Ref) { return visit(Ref->getPointee()); })
      .Default([&](const Type * /*T*/) {
        return true; // ErrTy, VarTy, BuiltinTy
      });
}

} // namespace phi
