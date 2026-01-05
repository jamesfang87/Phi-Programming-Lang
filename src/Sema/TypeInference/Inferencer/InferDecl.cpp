#include "Sema/TypeInference/Inferencer.hpp"

#include <llvm/ADT/TypeSwitch.h>

#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

void TypeInferencer::visit(VarDecl &D) {
  if (!D.hasInit()) {
    return;
  }

  auto Res = Unifier.unify(D.getType(), visit(D.getInit()));
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
  assert(D.hasType() && "ParamDecls must have type annotations");
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
  assert(D.hasType() && "ParamDecls must have type annotations");
  assert(!D.getType().isVar() && "ParamDecls cannot be annotated as VarTy");
  assert(!D.getType().isErr() && "ParamDecls cannot be annotated as ErrTy");

  if (!D.hasInit()) {
    return;
  }

  auto Res = Unifier.unify(D.getType(), D.getInit().getType());
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
  assert(D.getType().isAdt());

  for (auto &Field : D.getFields()) {
    visit(Field);
  }

  for (auto &Method : D.getMethods()) {
    visit(Method);
  }
}

void TypeInferencer::visit(EnumDecl &D) {
  assert(D.getType().isAdt());

  for (auto &Variant : D.getVariants()) {
    visit(Variant);
  }

  for (auto &Method : D.getMethods()) {
    visit(Method);
  }
}

void TypeInferencer::visit(VariantDecl &D) { (void)D; }

void TypeInferencer::visit(Decl &D) {
  llvm::TypeSwitch<Decl *>(&D)
      .Case<VarDecl>([&](VarDecl *X) { visit(*X); })
      .Case<ParamDecl>([&](ParamDecl *X) { visit(*X); })
      .Case<FunDecl>([&](FunDecl *X) { visit(*X); })
      .Case<FieldDecl>([&](FieldDecl *X) { visit(*X); })
      .Case<MethodDecl>([&](MethodDecl *X) { visit(*X); })
      .Case<StructDecl>([&](StructDecl *X) { visit(*X); })
      .Case<EnumDecl>([&](EnumDecl *X) { visit(*X); })
      .Default([&](Decl *) {
        llvm_unreachable("Unhandled Decl kind in TypeInferencer");
      });
}

} // namespace phi
