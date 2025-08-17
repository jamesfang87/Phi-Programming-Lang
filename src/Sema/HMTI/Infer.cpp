// src/Sema/HMTI/Infer.cpp
#include "Sema/HMTI/Infer.hpp"
#include "AST/Decl.hpp"
#include "Sema/HMTI/Adapters/TypeAdapters.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <memory>
#include <stdexcept>
#include <vector>

namespace phi {

// ----- token-kind helpers (adapt to actual TokenKind enum) -----
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

// ---------------- top-level driver ----------------
std::vector<std::unique_ptr<Decl>> TypeInferencer::inferProgram() {
  // Predeclare top-level vars and function names
  predeclare();

  // Infer each top-level declaration
  for (auto &Up : Ast_)
    inferDecl(*Up);

  defaultFloatLiterals();
  // Default integer literal variables that remain unconstrained (i32)
  defaultIntegerLiterals();

  checkIntegerConstraints(); // Add this line

  // Finalize annotations: apply global substitution and write back to AST
  finalizeAnnotations();
  return std::move(Ast_);
}

// ---------------- predeclaration ----------------
// Binds top-level VarDecl by ValueDecl pointer and FunDecl by name with their
// user-provided annotations (functions must be annotated).
void TypeInferencer::predeclare() {
  for (auto &Up : Ast_) {
    if (auto *V = dynamic_cast<VarDecl *>(Up.get())) {
      // give letrec a fresh variable
      auto Tv = Monotype::var(Factory_.fresh());
      Env_.bind(V, Polytype{{}, Tv});
    } else if (auto *F = dynamic_cast<FunDecl *>(Up.get())) {
      // Functions require parameter and return annotations by design.
      std::vector<std::shared_ptr<Monotype>> ArgTys;
      ArgTys.reserve(F->getParams().size());
      for (auto &Pup : F->getParams()) {
        ParamDecl *P = Pup.get();
        if (!P->hasType()) {
          throw std::runtime_error("Function parameter '" + P->getId() +
                                   "' must have a type annotation");
        }
        ArgTys.push_back(::phi::fromAstType(P->getType()));
      }
      // return type must be provided by user
      auto Ret = ::phi::fromAstType(F->getReturnTy());
      auto FnT = Monotype::fun(std::move(ArgTys), Ret);

      // Bind function by name (string) in the environment
      Env_.bind(F->getId(), generalize(Env_, FnT));
    }
  }
}

// ---------------- record / propagate substitutions ----------------
void TypeInferencer::recordSubst(const Substitution &S) {
  if (S.empty())
    return;
  // Compose new substitution into the global substitution (this := S ∘ this)
  GlobalSubst_.compose(S);
  // Also apply substitution to the environment for subsequent lookups
  Env_.applySubstitution(S);
}

// ---------------- adapters / annotation side-table ----------------
std::shared_ptr<Monotype>
TypeInferencer::typeFromAstOrFresh(std::optional<Type> AstTyOpt) {
  if (!AstTyOpt.has_value())
    return Monotype::var(Factory_.fresh());
  return ::phi::fromAstType(*AstTyOpt);
}

std::shared_ptr<Monotype> TypeInferencer::fromAstType(const Type &T) {
  return ::phi::fromAstType(T);
}
Type TypeInferencer::toAstType(const std::shared_ptr<Monotype> &T) {
  return ::phi::toAstType(T);
}

void TypeInferencer::annotate(ValueDecl &D,
                              const std::shared_ptr<Monotype> &T) {
  DeclMonos_[&D] = T;
}

void TypeInferencer::annotateExpr(Expr &E, const std::shared_ptr<Monotype> &T) {
  ExprMonos_[&E] = T;
}

// ---------------- integer defaulting ----------------
void TypeInferencer::defaultIntegerLiterals() {
  if (IntLiteralVars_.empty())
    return;

  Substitution S;
  for (auto &V : IntLiteralVars_) {
    // Only default if V is not yet bound in GlobalSubst_
    // If GlobalSubst_.Map contains V we avoid overwriting.
    if (GlobalSubst_.Map.find(V) == GlobalSubst_.Map.end()) {
      S.Map.emplace(V, Monotype::con("i32"));
    }
  }
  if (!S.empty())
    recordSubst(S);
}

void TypeInferencer::defaultFloatLiterals() {
  if (FloatLiteralVars_.empty())
    return;

  Substitution S;
  for (auto &V : FloatLiteralVars_) {
    // Only default if V is not yet bound in GlobalSubst_
    // If GlobalSubst_.Map contains V we avoid overwriting.
    if (GlobalSubst_.Map.find(V) == GlobalSubst_.Map.end()) {
      S.Map.emplace(V, Monotype::con("f32"));
    }
  }
  if (!S.empty())
    recordSubst(S);
}

// ---------------- finalize annotations ----------------
// Apply GlobalSubst_ to stored monotypes and write back into AST nodes.
void TypeInferencer::finalizeAnnotations() {
  // Finalize ValueDecls
  for (auto &KV : DeclMonos_) {
    const ValueDecl *VD = KV.first;
    auto Mono = GlobalSubst_.apply(KV.second);
    Type AstTy = toAstType(Mono);
    // ValueDecl::setType exists; AST ownership allows mutation here.
    const_cast<ValueDecl *>(VD)->setType(std::move(AstTy));
  }

  // Finalize Exprs
  for (auto &KV : ExprMonos_) {
    const Expr *E = KV.first;
    auto Mono = GlobalSubst_.apply(KV.second);
    Type AstTy = toAstType(Mono);
    const_cast<Expr *>(E)->setType(std::move(AstTy));
  }

  // Optionally clear side tables
  ExprMonos_.clear();
  DeclMonos_.clear();
}

} // namespace phi
