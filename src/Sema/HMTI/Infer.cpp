#include "Sema/HMTI/Infer.hpp"
#include "AST/Decl.hpp"
#include "Sema/HMTI/Algorithms.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <llvm/Support/Casting.h>

#include <memory>
#include <vector>

namespace phi {

// ---------------- constructor ----------------
TypeInferencer::TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast)
    : Ast(std::move(Ast)) {
  for (const auto &D : this->Ast) {
  }
}

// ---------------- top-level driver ----------------
std::vector<std::unique_ptr<Decl>> TypeInferencer::inferProgram() {
  // Predeclare top-level vars and function names
  predeclare();

  // Infer each top-level declaration
  for (auto &Decl : Ast)
    inferDecl(*Decl);

  // Finalize annotations: apply global substitution and write back to AST
  finalizeAnnotations();
  return std::move(Ast);
}

// ---------------- predeclaration ----------------
// Binds top-level VarDecl by ValueDecl pointer and FunDecl by name with their
// user-provided annotations (functions must be annotated).
void TypeInferencer::predeclare() {
  for (auto &Decl : Ast) {
    if (auto Var = llvm::dyn_cast<VarDecl>(Decl.get())) {
      auto NewTypeVar = Monotype::makeVar(Factory.fresh());
      Env.bind(Var, Polytype{{}, NewTypeVar});
      continue;
    }

    if (auto Fun = llvm::dyn_cast<FunDecl>(Decl.get())) {
      std::vector<Monotype> ParamTypes;
      ParamTypes.reserve(Fun->getParams().size());

      for (auto &Param : Fun->getParams()) {
        assert(Param->hasType());
        ParamTypes.push_back(Param->getType().toMonotype());
      }

      auto Ret = Fun->getReturnTy().toMonotype();
      auto FunType = Monotype::makeFun(std::move(ParamTypes), Ret);
      Env.bind(Fun->getId(), generalize(Env, FunType));
    }

    if (auto Struct = llvm::dyn_cast<StructDecl>(Decl.get())) {
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
  for (auto &V : IntTypeVars) {
    if (GlobalSubst.Map.contains(V)) {
      continue;
    }

    if (V.Constraints && !std::ranges::contains(*V.Constraints, "i32")) {
      continue;
    }

    Substitution S;
    S.Map.emplace(V, Monotype::makeCon("i32"));
    recordSubst(S);
  }

  // Similar logic for floats
  for (auto &V : FloatTypeVars) {
    if (GlobalSubst.Map.contains(V)) {
      continue;
    }

    if (V.Constraints && !std::ranges::contains(*V.Constraints, "f32")) {
      continue;
    }

    Substitution S;
    S.Map.emplace(V, Monotype::makeCon("f32"));
    recordSubst(S);
  }
}

// ---------------- finalize annotations ----------------
// Apply GlobalSubst_ to stored monotypes and write back into AST nodes.
void TypeInferencer::finalizeAnnotations() {
  defaultNums(); // default floats and ints to f32, i32

  // Finalize ValueDecls
  for (auto &Decl : ValDeclMonos) {
    Monotype T = GlobalSubst.apply(Decl.second);
    Decl.first->setType(T.toAstType());
  }

  // Finalize Exprs
  for (auto &Expr : ExprMonos) {
    Monotype T = GlobalSubst.apply(Expr.second);
    Expr.first->setType(T.toAstType());
  }

  // Optionally clear side tables
  ExprMonos.clear();
  ValDeclMonos.clear();
}

// ----- token-kind helpers -----
bool TypeInferencer::isArithmetic(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::PlusKind:
  case TokenKind::MinusKind:
  case TokenKind::StarKind:
  case TokenKind::SlashKind:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isLogical(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::DoubleAmpKind:
  case TokenKind::DoublePipeKind:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isComparison(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::OpenCaretKind:
  case TokenKind::LessEqualKind:
  case TokenKind::CloseCaretKind:
  case TokenKind::GreaterEqualKind:
    return true;
  default:
    return false;
  }
}

bool TypeInferencer::isEquality(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::DoubleEqualsKind:
  case TokenKind::BangEqualsKind:
    return true;
  default:
    return false;
  }
}

} // namespace phi
