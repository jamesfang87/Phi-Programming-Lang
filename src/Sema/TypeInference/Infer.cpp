#include "Sema/TypeInference/Infer.hpp"

#include <memory>
#include <optional>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"
#include "Sema/TypeInference/Algorithms.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"

namespace phi {

// ---------------- constructor ----------------
TypeInferencer::TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast)
    : Ast(std::move(Ast)) {}

// ---------------- top-level driver ----------------
std::vector<std::unique_ptr<Decl>> TypeInferencer::inferProgram() {
  // Predeclare top-level vars and function names
  predeclare();

  // Infer each top-level declaration
  for (auto &Decl : Ast)
    visit(*Decl);

  // Finalize annotations: apply global substitution and write back to AST
  finalizeAnnotations();
  return std::move(Ast);
}

// ---------------- predeclaration ----------------
// Binds top-level VarDecl by ValueDecl pointer and FunDecl by name with their
// user-provided annotations (functions must be annotated).
void TypeInferencer::predeclare() {
  for (auto &Decl : Ast) {
    if (const auto Var = llvm::dyn_cast<VarDecl>(Decl.get())) {
      const auto NewTypeVar = Monotype::makeVar(Factory.fresh());
      Env.bind(Var, Polytype{{}, NewTypeVar});
      continue;
    }

    if (const auto Fun = llvm::dyn_cast<FunDecl>(Decl.get())) {
      std::vector<Monotype> ParamTypes;
      ParamTypes.reserve(Fun->getParams().size());

      for (const auto &Param : Fun->getParams()) {
        assert(Param->hasType());
        ParamTypes.push_back(Param->getType().toMonotype());
      }

      const auto Ret = Fun->getReturnTy().toMonotype();
      auto FunType = Monotype::makeFun(std::move(ParamTypes), Ret);
      Env.bind(Fun->getId(), generalize(Env, FunType));
    }

    if (const auto Struct = llvm::dyn_cast<StructDecl>(Decl.get())) {
      Structs[Struct->getId()] = Struct;
    }
  }
}

// ---------------- record / propagate substitutions ----------------
void TypeInferencer::recordSubst(const Substitution &S) {
  if (S.empty())
    return;

  // Compose new substitution into the global substitution (this := S ∘ this)
  GlobalSubst.compose(S);
  // Also apply substitution to the environment for subsequent lookups
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
  auto tryDefault = [&](auto &TypeVarList, const std::string &TargetCon) {
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
          !std::ranges::contains(*RepVar.Constraints, TargetCon)) {
        continue;
      }

      // Default the representative to the concrete type (i32 / f32).
      Substitution S;
      S.Map.emplace(RepVar, Monotype::makeCon(TargetCon));
      recordSubst(S);
    }
  };

  tryDefault(IntTypeVars, "i32");
  tryDefault(FloatTypeVars, "f32");
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

  // Optionally clear side tables
  ExprMonos.clear();
  ValDeclMonos.clear();
}

// ----- token-kind helpers -----
bool TypeInferencer::isArithmetic(const TokenKind K) noexcept {
  switch (K) {
  case TokenKind::Plus:
  case TokenKind::Minus:
  case TokenKind::Star:
  case TokenKind::Slash:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isLogical(const TokenKind K) noexcept {
  switch (K) {
  case TokenKind::DoubleAmp:
  case TokenKind::DoublePipe:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isComparison(const TokenKind K) noexcept {
  switch (K) {
  case TokenKind::OpenCaret:
  case TokenKind::LessEqual:
  case TokenKind::CloseCaret:
  case TokenKind::GreaterEqual:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isEquality(const TokenKind K) noexcept {
  switch (K) {
  case TokenKind::DoubleEquals:
  case TokenKind::BangEquals:
    return true;
  default:
    return false;
  }
}

} // namespace phi
