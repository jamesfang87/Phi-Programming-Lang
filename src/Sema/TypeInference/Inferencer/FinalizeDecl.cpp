#include "Sema/TypeInference/Inferencer.hpp"

#include <llvm/ADT/TypeSwitch.h>

namespace phi {

void TypeInferencer::finalize(Decl &D) {
  llvm::TypeSwitch<Decl *>(&D)
      .Case<VarDecl>([&](VarDecl *X) { finalize(*X); })
      .Case<FunDecl>([&](FunDecl *X) { finalize(*X); })
      .Case<MethodDecl>([&](MethodDecl *X) { finalize(*X); })
      .Case<StructDecl>([&](StructDecl *X) { finalize(*X); })
      .Case<EnumDecl>([&](EnumDecl *X) { finalize(*X); })
      .Case<ModuleDecl>([&](ModuleDecl *X) { finalize(*X); })
      .Default([&](Decl *) {});
}

void TypeInferencer::finalize(VarDecl &D) {
  if (D.hasInit()) {
    finalize(D.getInit());
  }
  D.setType(Unifier.resolve(D.getType()));
}

void TypeInferencer::finalize(FunDecl &D) { finalize(D.getBody()); }

void TypeInferencer::finalize(MethodDecl &D) { finalize(D.getBody()); }

void TypeInferencer::finalize(StructDecl &D) {
  for (auto &Method : D.getMethods()) {
    finalize(*Method);
  }
}

void TypeInferencer::finalize(EnumDecl &D) {
  for (auto &Method : D.getMethods()) {
    finalize(*Method);
  }
}

void TypeInferencer::finalize(ModuleDecl &D) {
  for (auto &Decl : D.getItems()) {
    finalize(*Decl);
  }
}

} // namespace phi
