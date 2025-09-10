#include "Sema/TypeInference/Infer.hpp"

#include "AST/Stmt.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include <cassert>

namespace phi {

// ---------------- Block / Stmt ----------------
TypeInferencer::InferRes TypeInferencer::inferBlock(Block &B) {
  Substitution AllSubsts;
  for (const auto &Stmt : B.getStmts()) {
    auto [StmtSubst, _] = visit(*Stmt);
    AllSubsts.compose(StmtSubst);
  }
  return {AllSubsts, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(Stmt &S) {
  return S.accept(*this);
}

TypeInferencer::InferRes TypeInferencer::visit(ReturnStmt &S) {
  const auto ExpectedType =
      CurFunRetType.empty() ? Monotype::makeCon("unit") : CurFunRetType.back();
  if (S.hasExpr()) {
    auto [Subst, ActualType] = visit(S.getExpr());
    unifyInto(Subst, ActualType, ExpectedType);
    recordSubst(Subst);
    return {Subst, Monotype::makeCon("unit")};
  } else {
    Substitution Subst;
    unifyInto(Subst, Monotype::makeCon("unit"), ExpectedType);
    recordSubst(Subst);
    return {Subst, Monotype::makeCon("unit")};
  }
}

TypeInferencer::InferRes TypeInferencer::visit(DeferStmt &S) {
  Substitution Subst;
  auto [DeferredSubst, _] = visit(S.getDeferred());
  Subst.compose(DeferredSubst);

  recordSubst(Subst);

  return {Subst, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(ForStmt &S) {
  // 1) Infer the range expression (we don’t constrain its type here)
  auto [RangeSubst, RangeType] = visit(S.getRange());
  Substitution AllSubsts = RangeSubst;

  // 2) Get the loop variable
  VarDecl *LoopVar = &S.getLoopVar();
  assert(LoopVar);

  // 3) Create fresh type variable for loop variable, restricted to integer
  // types
  assert(RangeType.asCon().Args.size() == 1);
  auto LoopVarTy = RangeType.asCon().Args[0];
  // IntTypeVars.push_back(LoopVarTy.asVar());

  // 4) Bind loop variable in environment
  recordSubst(AllSubsts);
  const auto BoundTy = AllSubsts.apply(LoopVarTy);
  Env.bind(LoopVar, Polytype{{}, BoundTy});
  annotate(*LoopVar, BoundTy);

  // 5) Infer the loop body
  auto [BlockSubst, _] = inferBlock(S.getBody());
  AllSubsts.compose(BlockSubst);
  recordSubst(BlockSubst);

  return {AllSubsts, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclStmt &S) {
  visit(S.getDecl());
  return {Substitution{}, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(WhileStmt &S) {
  auto [CondSubst, CondType] = visit(S.getCond());
  Substitution AllSubsts = CondSubst;

  auto [BlockSubst, _] = inferBlock(S.getBody());
  AllSubsts.compose(BlockSubst);

  recordSubst(AllSubsts);
  return {AllSubsts, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(IfStmt &S) {
  auto [CondSubst, CondType] = visit(S.getCond());
  Substitution AllSubsts = CondSubst;

  auto [ThenBlockSubst, _] = inferBlock(S.getThen());
  AllSubsts.compose(ThenBlockSubst);

  if (S.hasElse()) {
    auto [ElseBlockSubst, __] = inferBlock(S.getElse());
    AllSubsts.compose(ElseBlockSubst);
  }

  recordSubst(AllSubsts);
  return {AllSubsts, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(BreakStmt &S) {
  (void)S;
  return {Substitution{}, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(ContinueStmt &S) {
  (void)S;
  return {Substitution{}, Monotype::makeCon("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(ExprStmt &S) {
  return visit(S.getExpr());
}

} // namespace phi
