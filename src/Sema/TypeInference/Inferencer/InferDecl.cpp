#include "Sema/TypeInference/Inferencer.hpp"

#include <memory>
#include <unordered_map>

#include <llvm/ADT/TypeSwitch.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

void TypeInferencer::visit(Decl &D) {
  llvm::TypeSwitch<Decl *>(&D)
      .Case<VarDecl>([&](VarDecl *X) { visit(*X); })
      .Case<ParamDecl>([&](ParamDecl *X) { visit(*X); })
      .Case<FunDecl>([&](FunDecl *X) { visit(*X); })
      .Case<FieldDecl>([&](FieldDecl *X) { visit(*X); })
      .Case<MethodDecl>([&](MethodDecl *X) { visit(*X); })
      .Case<StructDecl>([&](StructDecl *X) { visit(*X); })
      .Case<EnumDecl>([&](EnumDecl *X) { visit(*X); })
      .Case<VariantDecl>([&](VariantDecl *X) { visit(*X); })
      .Case<ModuleDecl>([&](ModuleDecl *X) { visit(*X); })
      .Default([&](Decl *) {
        llvm_unreachable("Unhandled Decl kind in TypeInferencer");
      });
}

std::unordered_map<const TypeArgDecl *, TypeRef>
TypeInferencer::buildGenericSubstMap(
    const std::vector<std::unique_ptr<TypeArgDecl>> &TypeArgs) {
  std::unordered_map<const TypeArgDecl *, TypeRef> Map;
  for (auto &T : TypeArgs) {
    Map.emplace(T.get(), TypeCtx::getVar(VarTy::Any, T->getSpan()));
    assert(Map.contains(T.get()));
  }
  return Map;
}

TypeRef TypeInferencer::substituteGenerics(
    TypeRef Ty, const std::unordered_map<const TypeArgDecl *, TypeRef> &Map) {
  if (Map.empty()) {
    return Ty;
  }

  if (auto G = llvm::dyn_cast<GenericTy>(Ty.getPtr())) {
    auto It = Map.find(G->getDecl());
    if (It != Map.end()) {
      return It->second;
    }
    return Ty;
  }

  if (auto A = llvm::dyn_cast<AppliedTy>(Ty.getPtr())) {
    std::vector<TypeRef> SubstitutedArgs;
    for (auto &ArgTy : A->getArgs()) {
      SubstitutedArgs.push_back(substituteGenerics(ArgTy, Map));
    }
    return TypeCtx::getApplied(A->getBase(), SubstitutedArgs, Ty.getSpan());
  }

  return Ty;
}

TypeRef TypeInferencer::instantiate(Decl *D) {
  return llvm::TypeSwitch<Decl *, TypeRef>(D)
      .Case<LocalDecl>([&](LocalDecl *X) {
        if (X->getType().isGeneric()) {
          return TypeCtx::getVar(VarTy::Any, X->getType().getSpan());
        }
        return X->getType();
      })
      .Case<FunDecl>([&](FunDecl *X) {
        auto Map = buildGenericSubstMap(X->getTypeArgs());

        std::vector<TypeRef> ParamTs;
        for (auto &Param : X->getParams()) {
          ParamTs.emplace_back(substituteGenerics(Param->getType(), Map));
        }
        TypeRef ReturnT = substituteGenerics(X->getReturnType(), Map);
        return TypeCtx::getFun(std::move(ParamTs), std::move(ReturnT),
                               X->getSpan());
      })
      .Case<MethodDecl>([&](MethodDecl *X) {
        auto Map = buildGenericSubstMap(X->getTypeArgs());
        Map.merge(buildGenericSubstMap(X->getParent()->getTypeArgs()));

        std::vector<TypeRef> ParamTs;
        for (auto &Param : X->getParams()) {
          ParamTs.emplace_back(substituteGenerics(Param->getType(), Map));
        }
        TypeRef ReturnT = substituteGenerics(X->getReturnType(), Map);
        return TypeCtx::getFun(std::move(ParamTs), std::move(ReturnT),
                               X->getSpan());
      })
      .Case<FieldDecl>([&](FieldDecl *X) {
        if (X->getType().isGeneric()) {
          return TypeCtx::getVar(VarTy::Any, X->getType().getSpan());
        }
        return X->getType();
      })
      .Case<VariantDecl>([&](VariantDecl *X) {
        assert(
            X->hasPayload() &&
            "`TypeInferencer::instantiate` called on payload-less VariantDecl");

        if (X->getPayloadType().isGeneric()) {
          return TypeCtx::getVar(VarTy::Any, X->getPayloadType().getSpan());
        }
        return X->getPayloadType();
      })
      .Case<AdtDecl>([&](AdtDecl *X) {
        if (!X->hasTypeArgs()) {
          return X->getType();
        }

        std::vector<TypeRef> Vars;
        for (auto &Generic : X->getTypeArgs()) {
          Vars.push_back(TypeCtx::getVar(VarTy::Any, Generic->getSpan()));
        }
        return TypeCtx::getApplied(X->getType(), std::move(Vars), X->getSpan());
      })
      .Case<ModuleDecl>([&](ModuleDecl *X) {
        static_assert("`TypeInferencer::instantiate called on ModuleDecl, "
                      "which has no type");
        return TypeCtx::getErr(X->getSpan());
      })
      .Default([&](Decl *X) {
        llvm_unreachable("Unhandled Decl kind in TypeInferencer");
        return TypeCtx::getErr(X->getSpan());
      });
}

void TypeInferencer::visit(VarDecl &D) {
  if (!D.hasInit()) {
    return;
  }

  auto T = instantiate(&D);
  auto Res = Unifier.unify(T, visit(D.getInit()));
  if (!Res) {
    error("Mismatched types in variable declaration")
        .with_primary_label(D.getInit().getSpan(),
                            std::format("expected this to be {}, not {}",
                                        toString(D.getType()),
                                        toString(D.getInit().getType())))
        .with_secondary_label(D.getType().getSpan(), "due to this")
        .emit(*DiagMan);
    return;
  }
}

void TypeInferencer::visit(ParamDecl &D) {
  assert(!D.getType().isVar() && "ParamDecls cannot be annotated as VarTy");
  assert(!D.getType().isErr() && "ParamDecls cannot be annotated as ErrTy");
}

void TypeInferencer::visit(FunDecl &D) {
  CurrentFun = &D;
  for (auto &Param : D.getParams()) {
    visit(*Param);
  }

  visit(D.getBody());
}

void TypeInferencer::visit(FieldDecl &D) {
  assert(!D.getType().isVar() && "FieldDecls cannot be annotated as VarTy");
  assert(!D.getType().isErr() && "FieldDecls cannot be annotated as ErrTy");

  if (!D.hasInit()) {
    return;
  }

  auto T = instantiate(&D);
  auto Res = Unifier.unify(T, D.getInit().getType());
  if (!Res) {
    error("Mismatched types in field declaration")
        .with_primary_label(D.getInit().getSpan(),
                            std::format("expected this to be {}, not {}",
                                        toString(D.getType()),
                                        toString(D.getInit().getType())))
        .with_secondary_label(D.getSpan(), "due to this")
        .emit(*DiagMan);
  }
}

void TypeInferencer::visit(MethodDecl &D) {
  CurrentFun = &D;
  for (auto &Param : D.getParams()) {
    visit(*Param);
  }

  visit(D.getBody());
}

void TypeInferencer::visit(StructDecl &D) {
  for (auto &Field : D.getFields()) {
    visit(*Field);
  }

  for (auto &Method : D.getMethods()) {
    visit(*Method);
  }
}

void TypeInferencer::visit(EnumDecl &D) {
  for (auto &Variant : D.getVariants()) {
    visit(*Variant);
  }

  for (auto &Method : D.getMethods()) {
    visit(*Method);
  }
}

void TypeInferencer::visit(VariantDecl &D) {
  if (!D.hasPayload()) {
    return;
  }

  assert(!D.getPayloadType().isVar() &&
         "FieldDecls cannot be annotated as VarTy");
  assert(!D.getPayloadType().isErr() &&
         "FieldDecls cannot be annotated as ErrTy");
}

void TypeInferencer::visit(ModuleDecl &D) {
  for (auto &Decl : D.getItems()) {
    visit(*Decl);
  }
}

} // namespace phi
