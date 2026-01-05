#include "Sema/TypeInference/Inferencer.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

void TypeInferencer::finalize(Expr &E) {
  llvm::TypeSwitch<Expr *>(&E)
      .Case<IntLiteral>([&](IntLiteral *X) { finalize(*X); })
      .Case<FloatLiteral>([&](FloatLiteral *X) { finalize(*X); })
      .Case<StrLiteral>([&](StrLiteral *X) { finalize(*X); })
      .Case<CharLiteral>([&](CharLiteral *X) { finalize(*X); })
      .Case<BoolLiteral>([&](BoolLiteral *X) { finalize(*X); })
      .Case<RangeLiteral>([&](RangeLiteral *X) { finalize(*X); })
      .Case<TupleLiteral>([&](TupleLiteral *X) { finalize(*X); })
      .Case<DeclRefExpr>([&](DeclRefExpr *X) { finalize(*X); })
      .Case<FunCallExpr>([&](FunCallExpr *X) { finalize(*X); })
      .Case<BinaryOp>([&](BinaryOp *X) { finalize(*X); })
      .Case<UnaryOp>([&](UnaryOp *X) { finalize(*X); })
      .Case<MemberInit>([&](MemberInit *X) { finalize(*X); })
      .Case<FieldAccessExpr>([&](FieldAccessExpr *X) { finalize(*X); })
      .Case<MethodCallExpr>([&](MethodCallExpr *X) { finalize(*X); })
      .Case<MatchExpr>([&](MatchExpr *X) { finalize(*X); })
      .Case<AdtInit>([&](AdtInit *X) { finalize(*X); })
      .Default([&](Expr *) {
        llvm_unreachable("Unhandled Expr kind in TypeInferencer");
      });
}

void TypeInferencer::finalize(IntLiteral &E) {
  auto T = Unifier.resolve(E.getType());
  assert(T.isBuiltin() || T.isVar());
  if (T.isVar()) {
    auto Int = llvm::dyn_cast<VarTy>(T.getPtr());
    assert(Int->getDomain() == VarTy::Int);
    Unifier.unify(E.getType(),
                  TypeCtx::getBuiltin(BuiltinTy::i32, E.getSpan()));
    E.setType(TypeCtx::getBuiltin(BuiltinTy::i32, E.getSpan()));
  } else {
    E.setType(T);
  }
}

void TypeInferencer::finalize(FloatLiteral &E) {
  auto T = Unifier.resolve(E.getType());
  assert(T.isBuiltin() || T.isVar());
  if (T.isVar()) {
    auto Float = llvm::dyn_cast<VarTy>(T.getPtr());
    assert(Float->getDomain() == VarTy::Float);
    E.setType(TypeCtx::getBuiltin(BuiltinTy::f64, E.getSpan()));
  } else {
    E.setType(T);
  }
}

void TypeInferencer::finalize(BoolLiteral &E) {
  assert(E.getType().getPtr() ==
         TypeCtx::getBuiltin(BuiltinTy::Bool, E.getSpan()).getPtr());
}

void TypeInferencer::finalize(CharLiteral &E) {
  assert(E.getType().getPtr() ==
         TypeCtx::getBuiltin(BuiltinTy::Char, E.getSpan()).getPtr());
}

void TypeInferencer::finalize(StrLiteral &E) {
  assert(E.getType().getPtr() ==
         TypeCtx::getBuiltin(BuiltinTy::String, E.getSpan()).getPtr());
}

void TypeInferencer::finalize(RangeLiteral &E) {
  finalize(E.getStart());
  finalize(E.getEnd());
  assert(E.getType().getPtr() ==
         TypeCtx::getBuiltin(BuiltinTy::Range, E.getSpan()).getPtr());
}

void TypeInferencer::finalize(TupleLiteral &E) {
  for (auto &Elem : E.getElements()) {
    finalize(*Elem);
  }
  E.setType(Unifier.resolve(E.getType()));
}

void TypeInferencer::finalize(DeclRefExpr &E) {
  E.setType(Unifier.resolve(E.getType()));
  assert(E.getType().getPtr() == E.getDecl()->getType().getPtr());
}

void TypeInferencer::finalize(FunCallExpr &E) {
  for (auto &Arg : E.getArgs()) {
    finalize(*Arg);
  }

  E.setType(Unifier.resolve(E.getType()));
  assert(E.getType().getPtr() == E.getDecl()->getReturnTy().getPtr());
}

void TypeInferencer::finalize(BinaryOp &E) {
  finalize(E.getLhs());
  finalize(E.getRhs());
  assert(E.getLhs().getType().getPtr() == E.getRhs().getType().getPtr());
  E.setType(Unifier.resolve(E.getType()));
}

void TypeInferencer::finalize(UnaryOp &E) {
  finalize(E.getOperand());
  E.setType(Unifier.resolve(E.getType()));
}

void TypeInferencer::finalize(AdtInit &E) {
  E.setType(Unifier.resolve(E.getType()));
  for (auto &Init : E.getInits()) {
    finalize(*Init);
  }
}

void TypeInferencer::finalize(MemberInit &E) {
  if (E.getInitValue()) {
    finalize(*E.getInitValue());
  }
}

void TypeInferencer::finalize(FieldAccessExpr &E) {
  E.setType(Unifier.resolve(E.getType()));
}

void TypeInferencer::finalize(MethodCallExpr &E) {
  for (auto &Arg : E.getArgs()) {
    finalize(*Arg);
  }

  E.setType(Unifier.resolve(E.getType()));
  assert(E.getType().getPtr() == E.getDecl()->getReturnTy().getPtr());
}

void TypeInferencer::finalize(MatchExpr &E) {
  // 1. Scrutinee must type-check
  finalize(*E.getScrutinee());
  auto ScrutineeT = E.getScrutinee()->getType().getUnderlying();

  // 2. Scrutinee must be matchable
  const EnumDecl *Enum = nullptr;
  if (ScrutineeT.isAdt()) {
    auto Decl = llvm::dyn_cast<AdtTy>(ScrutineeT.getPtr())->getDecl();
    Enum = llvm::dyn_cast<EnumDecl>(Decl);
  }

  if (Enum == nullptr && !ScrutineeT.isBuiltin()) {
    error("expression is not matchable")
        .with_primary_label(E.getSpan(), "cannot match on this type")
        .emit(*DiagMan);
    return;
  }

  // 3. Match must have arms
  auto &Arms = E.getArms();
  if (Arms.empty()) {
    error("match expression must have at least one arm")
        .with_primary_label(E.getSpan(), "empty match")
        .emit(*DiagMan);
    return;
  }

  assert(!Arms.empty());
  for (auto &Arm : Arms) {
    // 4. Check patterns
    for (auto &Pat : Arm.Patterns) {
      std::visit(
          [&](auto &P) {
            using PType = std::decay_t<decltype(P)>;

            // --- Wildcard ---
            if constexpr (std::is_same_v<PType, PatternAtomics::Wildcard>) {
            }

            // --- Literal pattern ---
            else if constexpr (std::is_same_v<PType, PatternAtomics::Literal>) {
              assert(P.Value);
              finalize(*P.Value);
              assert(P.Value->getType().getPtr() == ScrutineeT.getPtr());
            }

            // --- Variant pattern ---
            else if constexpr (std::is_same_v<PType, PatternAtomics::Variant>) {
              if (!Enum) {
                error("variant pattern used on non-enum type")
                    .with_primary_label(P.Location,
                                        "variant patterns require an enum")
                    .emit(*DiagMan);
              }

              VariantDecl *Variant = Enum->getVariant(P.VariantName);
              if (!Variant) {
                error("unknown enum variant")
                    .with_primary_label(P.Location, "no variant named `" +
                                                        P.VariantName + "`")
                    .emit(*DiagMan);
              }

              // Check payload arity
              if (Variant->hasType()) {
                auto PayloadTy = Variant->getType();

                if (P.Vars.size() != 1 && P.Vars.size() != 0) {
                  error("variant payload arity mismatch")
                      .with_primary_label(
                          P.Location, "expected 1 binding for variant payload")
                      .emit(*DiagMan);
                }

                if (P.Vars.size() == 0) {
                  return;
                }

                VarDecl *Binding = P.Vars.front().get();
                Unifier.unify(Binding->getType(), Variant->getType());
                // Check bound variable type
                finalize(*Binding);
                assert(Binding->hasType());

                if (Binding->getType().getPtr() != PayloadTy.getPtr()) {
                  error("variant binding type mismatch")
                      .with_primary_label(
                          Binding->getLocation(),
                          "binding type does not match variant payload")
                      .emit(*DiagMan);
                }
              } else {
                // Unit-like variant
                if (!P.Vars.empty()) {
                  error("variant has no payload")
                      .with_primary_label(P.Location,
                                          "this variant carries no data")
                      .emit(*DiagMan);
                }
              }
            }
          },
          Pat);
    }

    // 5. Arm body
    assert(Arm.Body);
    finalize(*Arm.Body);
  }
  E.setType(Unifier.resolve(E.getType()));
}

} // namespace phi
