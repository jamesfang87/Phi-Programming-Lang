#include "Sema/TypeInference/Inferencer.hpp"

namespace phi {

void TypeInferencer::finalize(VarDecl &D) {
  if (D.hasInit()) {
    finalize(D.getInit());
  }
  D.setType(Unifier.resolve(D.getType()));
}

void TypeInferencer::finalize(ParamDecl &D) { (void)D; }

void TypeInferencer::finalize(FunDecl &D) { finalize(D.getBody()); }

void TypeInferencer::finalize(FieldDecl &D) { (void)D; }

void TypeInferencer::finalize(MethodDecl &D) { finalize(D.getBody()); }

void TypeInferencer::finalize(StructDecl &D) {
  for (auto &Method : D.getMethods()) {
    finalize(Method);
  }
}

void TypeInferencer::finalize(EnumDecl &D) {
  for (auto &Method : D.getMethods()) {
    finalize(Method);
  }
}

void TypeInferencer::finalize(Decl &D) {
  llvm::TypeSwitch<Decl *>(&D)
      .Case<VarDecl>([&](VarDecl *X) { finalize(*X); })
      .Case<ParamDecl>([&](ParamDecl *X) { finalize(*X); })
      .Case<FunDecl>([&](FunDecl *X) { finalize(*X); })
      .Case<FieldDecl>([&](FieldDecl *X) { finalize(*X); })
      .Case<MethodDecl>([&](MethodDecl *X) { finalize(*X); })
      .Case<StructDecl>([&](StructDecl *X) { finalize(*X); })
      .Case<EnumDecl>([&](EnumDecl *X) { finalize(*X); })
      .Default([&](Decl *) {
        llvm_unreachable("Unhandled Decl kind in TypeInferencer");
      });
}

} // namespace phi
