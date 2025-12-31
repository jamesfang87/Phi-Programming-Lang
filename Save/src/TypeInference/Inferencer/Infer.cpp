#include "Sema/TypeInference/Infer.hpp"

#include <memory>
#include <optional>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"

namespace phi {

// ---------------- constructor ----------------
TypeInferencer::TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast,
                               std::shared_ptr<DiagnosticManager> DiagMan)
    : DiagMan(std::move(DiagMan)), Ast(std::move(Ast)) {}

// ---------------- top-level driver ----------------
std::vector<std::unique_ptr<Decl>> TypeInferencer::inferProgram() {
  predeclare();

  for (auto &Decl : Ast)
    visit(*Decl);

  finalizeAnnotations();
  return std::move(Ast);
}

// ---------------- predeclaration ----------------
void TypeInferencer::predeclare() {
  for (auto &Decl : Ast) {
    if (auto *const Fun = llvm::dyn_cast<FunDecl>(Decl.get())) {
      auto FunType = Fun->getType().toMonotype();
      Env.bind(Fun->getId(), FunType.generalize(Env));
    }

    if (auto *const Struct = llvm::dyn_cast<StructDecl>(Decl.get())) {
      Structs[Struct->getId()] = Struct;
    }

    if (auto *const Enum = llvm::dyn_cast<EnumDecl>(Decl.get())) {
      Enums[Enum->getId()] = Enum;
    }
  }
}

// ---------------- record / propagate substitutions ----------------
void TypeInferencer::recordSubst(const Substitution &S) {
  if (S.empty())
    return;

  GlobalSubst.compose(S);
  Env.applySubstitution(S);
}

// ---------------- annotation side-table ----------------
void TypeInferencer::annotate(ValueDecl &D, const Monotype &T) {
  ValDeclMonos[&D] = T;
}

void TypeInferencer::annotate(Expr &E, const Monotype &T) { ExprMonos[&E] = T; }

// ---------------- integer defaulting ----------------
void TypeInferencer::defaultNums() {
  // Helper lambda to default a list of type-vars to a concrete constructor
  auto tryDefault = [&](auto &TypeVarList, auto Default) {
    for (auto &V : TypeVarList) {
      // Apply the global substitution to get the current representative.
      // Build a monotype for V so we can apply the substitution:
      Monotype RepMono = GlobalSubst.apply(Monotype::makeVar(V));

      // If the representative is not a Var, it's already concrete -> skip.
      if (!RepMono.isVar())
        continue;

      // Get the representative variable (after applying global substs).
      TypeVar RepVar = RepMono.asVar();

      // If the representative already has been mapped in GlobalSubst
      // to something else, GlobalSubst.apply(...) above would have returned
      // that something else; we already handled that by the isVar() check.

      // If this representative has constraints and the target isn't allowed,
      // skip it.
      if (RepVar.Constraints &&
          !std::ranges::contains(*RepVar.Constraints,
                                 BuiltinKindToString(Default))) {
        continue;
      }

      // Default the representative to the concrete type (i32 / f64).
      Substitution S;
      S.Map.emplace(RepVar, Monotype::makeCon(Default));
      recordSubst(S);
    }
  };

  tryDefault(IntTypeVars, BuiltinKind::I32);
  tryDefault(FloatTypeVars, BuiltinKind::F64);
}

// ---------------- finalize annotations ----------------
// Apply GlobalSubst_ to stored monotypes and write back into AST nodes.
void TypeInferencer::finalizeAnnotations() {
  defaultNums(); // default floats and ints to f32, i32

  // Finalize ValueDecls
  for (auto &[Decl, Mono] : ValDeclMonos) {
    Monotype T = GlobalSubst.apply(Mono);
    Decl->setType(T.toAstType());
  }

  // Finalize Exprs
  for (auto &[Expr, Mono] : ExprMonos) {
    Monotype T = GlobalSubst.apply(Mono);
    Expr->setType(T.toAstType());
  }
}

} // namespace phi
