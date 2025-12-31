#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeChecker.hpp"

#include "AST/Nodes/Expr.hpp"
#include <print>

namespace phi {

bool TypeChecker::visit(MatchExpr &E) {
  assert(E.hasType());

  bool Success = true;

  // 1. Scrutinee must type-check
  Expr *Scrutinee = E.getScrutinee();
  assert(Scrutinee);

  Success = visit(*Scrutinee) && Success;

  assert(Scrutinee->getType().isRef());
  assert(Scrutinee->getType().asRef().Pointee->isEnum());
  assert(Scrutinee->getType().asRef().Pointee->asEnum().TheDecl);

  const Type &ScrutineeTy = Scrutinee->getType().unwrap();

  // 2. Scrutinee must be matchable
  if (!ScrutineeTy.isEnum() && !ScrutineeTy.isPrimitive()) {
    error("expression is not matchable")
        .with_primary_label(E.getLocation(), "cannot match on this type")
        .emit(*Diag);
    return false;
  }

  EnumDecl *Enum = nullptr;
  if (ScrutineeTy.isEnum()) {
    assert(ScrutineeTy.asEnum().TheDecl != nullptr);
    Enum = ScrutineeTy.asEnum().TheDecl;
  }

  // 3. Match must have arms
  auto &Arms = E.getArms();
  if (Arms.empty()) {
    error("match expression must have at least one arm")
        .with_primary_label(E.getLocation(), "empty match")
        .emit(*Diag);
    return false;
  }

  const Type &MatchResultTy = E.getType();

  for (auto &Arm : Arms) {
    // 4. Check patterns
    for (auto &Pat : Arm.Patterns) {
      Success &= std::visit(
          [&](auto &P) -> bool {
            using PType = std::decay_t<decltype(P)>;

            // --- Wildcard ---
            if constexpr (std::is_same_v<PType, PatternAtomics::Wildcard>) {
              return true;
            }

            // --- Literal pattern ---
            else if constexpr (std::is_same_v<PType, PatternAtomics::Literal>) {
              assert(P.Value);
              bool Ok = visit(*P.Value);
              Ok &= (P.Value->getType() == ScrutineeTy);
              return Ok;
            }

            // --- Variant pattern ---
            else if constexpr (std::is_same_v<PType, PatternAtomics::Variant>) {
              if (!Enum) {
                error("variant pattern used on non-enum type")
                    .with_primary_label(P.Location,
                                        "variant patterns require an enum")
                    .emit(*Diag);
                return false;
              }

              VariantDecl *Var = Enum->getVariant(P.VariantName);
              if (!Var) {
                error("unknown enum variant")
                    .with_primary_label(P.Location, "no variant named `" +
                                                        P.VariantName + "`")
                    .emit(*Diag);
                return false;
              }

              // Check payload arity
              if (Var->hasType()) {
                const Type &PayloadTy = Var->getType();

                if (P.Vars.size() != 1) {
                  error("variant payload arity mismatch")
                      .with_primary_label(
                          P.Location, "expected 1 binding for variant payload")
                      .emit(*Diag);
                  return false;
                }

                // Check bound variable type
                VarDecl *Binding = P.Vars.front().get();
                assert(Binding->hasType());

                if (Binding->getType() != PayloadTy) {
                  error("variant binding type mismatch")
                      .with_primary_label(
                          Binding->getLocation(),
                          "binding type does not match variant payload")
                      .emit(*Diag);
                  return false;
                }
              } else {
                // Unit-like variant
                if (!P.Vars.empty()) {
                  error("variant has no payload")
                      .with_primary_label(P.Location,
                                          "this variant carries no data")
                      .emit(*Diag);
                  return false;
                }
              }

              return true;
            }
          },
          Pat);
    }

    // 5. Arm body must type-check
    assert(Arm.Body);
    Success = visit(*Arm.Body) && Success;

    // 6. Arm return type must match match expression type
    Success = (Arm.Return->getType() == MatchResultTy) && Success;
  }

  return Success;
}

} // namespace phi
