#include "Sema/TypeInference/Unifier.hpp"

#include <optional>
#include <print>

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>
#include <llvm/Support/ErrorHandling.h>

#include "AST/TypeSystem/Type.hpp"

namespace phi {

TypeUnifier::Node &TypeUnifier::getNode(TypeRef T) {
  return getNode(T.getPtr());
}

TypeUnifier::Node &TypeUnifier::getNode(Type *T) {
  auto [It, Inserted] = Nodes.try_emplace(
      T, Node{T, T, 1,
              llvm::isa<VarTy>(T) ? llvm::cast<VarTy>(T)->getDomain()
                                  : VarTy::Domain::Any});
  return It->second;
}

Type *TypeUnifier::find(TypeRef T) { return find(T.getPtr()); }

Type *TypeUnifier::find(Type *T) {
  Node &N = Nodes.find(T)->second;
  return N.Parent == T ? T : (N.Parent = find(N.Parent));
}

bool TypeUnifier::unify(TypeRef A, TypeRef B) {
  assert(A.getPtr());
  assert(B.getPtr());

  // make sure both are inside the TypeUnifier context
  getNode(A);
  getNode(B);

  A = resolve(A);
  B = resolve(B);

  if (A.isErr() || B.isErr()) {
    return true;
  }

  if (A.isVar() && B.isVar()) {
    return unifyVars(A, B);
  }

  if (!A.isVar() && !B.isVar()) {
    return unifyConcretes(A, B);
  }

  if (!A.isVar()) {
    std::swap(A, B);
  }
  return unifyVarAndConcrete(A, B);
}

bool TypeUnifier::unifyVars(TypeRef A, TypeRef B) {
  auto *VarA = llvm::dyn_cast<VarTy>(A.getPtr());
  auto *VarB = llvm::dyn_cast<VarTy>(B.getPtr());
  assert(VarA && VarB);

  auto NewDomain = VarA->unifyDomain(VarB);
  if (!NewDomain) {
    return false;
  }

  Type *RootA = find(A);
  Type *RootB = find(B);
  assert(RootA && RootB);

  if (RootA == RootB) {
    return true;
  }

  auto ItA = Nodes.find(RootA);
  auto ItB = Nodes.find(RootB);
  auto *RootNodeA = &ItA->second;
  auto *RootNodeB = &ItB->second;

  // RootA always has the larger size
  if (RootNodeA->Size < RootNodeB->Size) {
    std::swap(RootA, RootB);
    std::swap(RootNodeA, RootNodeB);
  }

  // RootA becomes root of all nodes under RootB
  RootNodeA->Size += RootNodeB->Size;
  RootNodeB->Parent = RootA;

  // Make sure to update the domain
  RootNodeA->Domain = *NewDomain;
  RootNodeB->Domain = *NewDomain;

  if (auto *RootVar = llvm::dyn_cast<VarTy>(RootA)) {
    RootVar->setDomain(*NewDomain);
  }

  if (auto *OtherVar = llvm::dyn_cast<VarTy>(RootB)) {
    OtherVar->setDomain(*NewDomain);
  }

  return true;
}

bool TypeUnifier::unifyConcretes(TypeRef A, TypeRef B) {
  if (A.getPtr()->getKind() != B.getPtr()->getKind()) {
    return false;
  }

  return llvm::TypeSwitch<const Type *, bool>(A.getPtr())
      .Case<AdtTy>([&](const AdtTy *Adt) {
        auto Other = llvm::dyn_cast<AdtTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        return Adt->getId() == Other->getId();
      })
      .Case<TupleTy>([&](const TupleTy *Tuple) {
        auto Other = llvm::dyn_cast<TupleTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        const auto &ElemsA = Tuple->getElementTys();
        const auto &ElemsB = Other->getElementTys();

        if (ElemsA.size() != ElemsB.size()) {
          return false;
        }

        bool Res = true;
        for (auto [TypeA, TypeB] : llvm::zip(ElemsA, ElemsB)) {
          Res = unify(TypeA, TypeB) && Res;
        }

        return Res;
      })
      .Case<FunTy>([&](const FunTy *Fun) {
        auto Other = llvm::dyn_cast<FunTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        if (!unify(Fun->getReturnTy(), Other->getReturnTy())) {
          return false;
        }

        const auto &ParamsA = Fun->getParamTys();
        const auto &ParamsB = Other->getParamTys();

        if (ParamsA.size() != ParamsB.size()) {
          return false;
        }

        auto Res = true;
        for (auto [TypeA, TypeB] : llvm::zip(ParamsA, ParamsB)) {
          Res = unify(TypeA, TypeB) && Res;
        }

        return Res;
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
      .Case<BuiltinTy>([&](const BuiltinTy *Builtin) {
        auto Other = llvm::dyn_cast<BuiltinTy>(B.getPtr());
        assert(Other && "Types must be same kind at this point");

        return Builtin->getBuiltinKind() == Other->getBuiltinKind();
      })
      .Case<AppliedTy>([&](const AppliedTy *App) {
        auto Other = llvm::dyn_cast<AppliedTy>(B.getPtr());
        if (!unify(App->getBase(), Other->getBase())) {
          return false;
        }

        const auto &ArgsA = App->getArgs();
        const auto &ArgsB = Other->getArgs();

        if (ArgsA.size() != ArgsB.size()) {
          return false;
        }

        auto Res = true;
        for (auto [TypeA, TypeB] : llvm::zip(ArgsA, ArgsB)) {
          Res = unify(TypeA, TypeB) && Res;
        }

        return Res;
      })
      .Case<GenericTy>([&](const GenericTy *Generic) { return false; })
      .Default([](const Type * /*T*/) {
        llvm_unreachable("Unaccounted for Type* in Unifier");
        return false;
      });
}

bool TypeUnifier::unifyVarAndConcrete(TypeRef Var, TypeRef Con) {
  assert(Var.getPtr()->isVar());
  assert(!Con.getPtr()->isVar() && !Con.getPtr()->isErr());

  if (auto X = llvm::dyn_cast<VarTy>(Var.getPtr()); !X->accepts(Con)) {
    return false;
  }

  Type *VarRoot = find(Var);
  Type *ConRoot = find(Con);
  assert(VarRoot && ConRoot);

  if (VarRoot == ConRoot) {
    return true;
  }

  auto VarIt = Nodes.find(VarRoot);
  auto ConIt = Nodes.find(ConRoot);
  auto &VarRootNode = VarIt->second;
  auto &ConRootNode = ConIt->second;

  // RootA becomes root of all nodes under RootB
  ConRootNode.Size += VarRootNode.Size;
  VarRootNode.Parent = ConRoot;

  // Make sure to update the domain
  ConRootNode.Domain = std::nullopt;
  return true;
}

void TypeUnifier::emit() const {
  for (auto N : Nodes) {
    if (!N.getSecond().TheType->isVar()) {
      continue;
    }

    std::println("Type: {} Parent: {}", N.getSecond().TheType->toString(),
                 N.getSecond().Parent->toString());
  }
}

} // namespace phi
