#include "AST/Stmt.hpp"
#include "Sema/HMTI/Infer.hpp"

namespace phi {

// ---------------- Block / Stmt ----------------
TypeInferencer::InferRes TypeInferencer::inferBlock(Block &B) {
  Substitution S;
  for (auto &Up : B.getStmts()) {
    auto [Si, _] = Up->accept(*this);
    S.compose(Si);
  }
  return {S, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(ReturnStmt &S) {
  auto Expected = CurrentFnReturnTy_.empty() ? Monotype::con("unit")
                                             : CurrentFnReturnTy_.back();
  if (S.hasExpr()) {
    auto [si, t] = visit(S.getExpr());
    unifyInto(si, t, Expected);
    recordSubst(si);
    return {si, Monotype::con("unit")};
  } else {
    Substitution s0;
    unifyInto(s0, Monotype::con("unit"), Expected);
    recordSubst(s0);
    return {s0, Monotype::con("unit")};
  }
}

TypeInferencer::InferRes TypeInferencer::visit(ForStmt &S) {
  // 1) Infer the range expression
  auto [sRange, tRange] = visit(S.getRange());
  Substitution s = sRange;

  // 2) Create a fresh element type variable for the loop variable
  auto ElemTy = Monotype::var(Factory_.fresh());

  // 3) Expect the range to be 'range<ElemTy>'
  auto expectedRange = Monotype::con("range", {ElemTy});
  unifyInto(s, tRange, expectedRange);

  // 4) Get the loop variable
  VarDecl *LoopVar = &S.getLoopVar();
  if (!LoopVar)
    throw std::runtime_error("internal: ForStmt missing loop variable");

  std::shared_ptr<Monotype> LoopVarTy;

  if (LoopVar->hasType()) {
    auto AnnTy = fromAstType(LoopVar->getType());
    // Check if annotated type is integer
    if (!isIntegerType(AnnTy)) {
      throw std::runtime_error(
          "Loop variable '" + LoopVar->getId() +
          "' must have integer type, got: " + monotypeToString(AnnTy));
    }
    LoopVarTy = AnnTy;
    unifyInto(s, ElemTy, LoopVarTy);
  } else {
    // Create fresh type variable constrained to integer types
    LoopVarTy = Monotype::var(Factory_.fresh());
    // Record for integer constraint checking
    IntRangeVars_.push_back({LoopVarTy->asVar(), S.getLocation()});
    unifyInto(s, ElemTy, LoopVarTy);
  }

  // 6) Bind loop variable in environment
  recordSubst(s);
  auto BoundTy = s.apply(LoopVarTy);
  Env_.bind(LoopVar, Polytype{{}, BoundTy});
  annotate(*LoopVar, BoundTy);

  // 7) Infer body with loop variable in environment
  auto [sBody, _] = inferBlock(S.getBody());
  s.compose(sBody);
  recordSubst(sBody);

  return {s, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclStmt &S) {
  inferDecl(S.getDecl());
  return {Substitution{}, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(WhileStmt &S) {
  auto [sc, tc] = visit(S.getCond());
  Substitution sacc = sc;
  unifyInto(sacc, tc, Monotype::con("bool"));
  recordSubst(sacc);
  auto [sb, _] = inferBlock(S.getBody());
  sacc.compose(sb);
  recordSubst(sacc);
  return {sacc, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(IfStmt &S) {
  auto [sc, tc] = visit(S.getCond());
  Substitution sacc = sc;
  unifyInto(sacc, tc, Monotype::con("bool"));
  recordSubst(sacc);
  auto [st, _] = inferBlock(S.getThen());
  sacc.compose(st);
  if (S.hasElse()) {
    auto [se, __] = inferBlock(S.getElse());
    sacc.compose(se);
  }
  recordSubst(sacc);
  return {sacc, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(BreakStmt &S) {
  (void)S;
  return {Substitution{}, Monotype::con("unit")};
}

TypeInferencer::InferRes TypeInferencer::visit(ContinueStmt &S) {
  (void)S;
  return {Substitution{}, Monotype::con("unit")};
}

} // namespace phi
