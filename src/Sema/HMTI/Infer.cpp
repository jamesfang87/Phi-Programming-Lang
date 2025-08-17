// src/Sema/HMTI/Infer.cpp
#include "Sema/HMTI/Infer.hpp"
#include "AST/Decl.hpp"
#include "Sema/HMTI/Adapters/TypeAdapters.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <llvm/Support/Casting.h>

#include <memory>
#include <stdexcept>
#include <vector>

namespace phi {

// ---------------- constructor ----------------
TypeInferencer::TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast)
    : Ast(std::move(Ast)) {
  for (const auto &D : this->Ast) {
    if (auto S = llvm::dyn_cast<StructDecl>(D.get())) {
      Structs[S->getId()] = S;
    }
  }
}

// ---------------- top-level driver ----------------
std::vector<std::unique_ptr<Decl>> TypeInferencer::inferProgram() {
  // Predeclare top-level vars and function names
  predeclare();

  // Infer each top-level declaration
  for (auto &Up : Ast)
    inferDecl(*Up);

  // Finalize annotations: apply global substitution and write back to AST
  finalizeAnnotations();
  return std::move(Ast);
}

// ---------------- predeclaration ----------------
// Binds top-level VarDecl by ValueDecl pointer and FunDecl by name with their
// user-provided annotations (functions must be annotated).
void TypeInferencer::predeclare() {
  for (auto &Up : Ast) {
    if (auto V = llvm::dyn_cast<VarDecl>(Up.get())) {
      // give letrec a fresh variable
      auto Tv = Monotype::var(Factory.fresh());
      Env.bind(V, Polytype{{}, Tv});
      continue;
    }

    if (auto F = llvm::dyn_cast<FunDecl>(Up.get())) {
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
      Env.bind(F->getId(), generalize(Env, FnT));
      continue;
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

// ---------------- adapters / annotation side-table ----------------
std::shared_ptr<Monotype>
TypeInferencer::typeFromAstOrFresh(std::optional<Type> AstTyOpt) {
  if (!AstTyOpt.has_value())
    return Monotype::var(Factory.fresh());
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
  ValDeclMonos[&D] = T;
}

void TypeInferencer::annotate(Expr &E, const std::shared_ptr<Monotype> &T) {
  ExprMonos[&E] = T;
}

// ---------------- integer defaulting ----------------
void TypeInferencer::defaultNums() {
  auto Nums = {std::pair{std::cref(FloatLiteralVars), "f32"},
               std::pair{std::cref(IntLiteralVars), "i32"}};

  // Loop through both floats and ints
  for (auto &[Vars, TypeName] : Nums) {
    Substitution S;
    for (auto &V : Vars.get()) {
      // Replace if it is not in the GlobalSubst
      if (!GlobalSubst.Map.contains(V)) {
        S.Map.emplace(V, Monotype::con(TypeName));
      }
    }
    recordSubst(S);
  }
}

// ---------------- finalize annotations ----------------
// Apply GlobalSubst_ to stored monotypes and write back into AST nodes.
void TypeInferencer::finalizeAnnotations() {
  defaultNums();             // default floats and ints to f32, i32
  checkIntegerConstraints(); // check integer constraints

  // Finalize ValueDecls
  for (auto &D : ValDeclMonos) {
    auto Mono = GlobalSubst.apply(D.second);
    D.first->setType(toAstType(Mono));
  }

  // Finalize Exprs
  for (auto &E : ExprMonos) {
    auto Mono = GlobalSubst.apply(E.second);
    E.first->setType(toAstType(Mono));
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
