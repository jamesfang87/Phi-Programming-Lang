#include "Sema/TypeInference/Inferencer.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

void TypeInferencer::visit(Stmt &S) {
  llvm::TypeSwitch<Stmt *>(&S)
      .Case<ReturnStmt>([&](ReturnStmt *X) { visit(*X); })
      .Case<DeferStmt>([&](DeferStmt *X) { visit(*X); })
      .Case<IfStmt>([&](IfStmt *X) { visit(*X); })
      .Case<WhileStmt>([&](WhileStmt *X) { visit(*X); })
      .Case<ForStmt>([&](ForStmt *X) { visit(*X); })
      .Case<DeclStmt>([&](DeclStmt *X) { visit(*X); })
      .Case<ContinueStmt>([&](ContinueStmt *X) { visit(*X); })
      .Case<BreakStmt>([&](BreakStmt *X) { visit(*X); })
      .Case<ExprStmt>([&](ExprStmt *X) { visit(*X); })
      .Default([&](Stmt *) { std::unreachable(); });
}

void TypeInferencer::visit(ReturnStmt &S) {
  if (!S.hasExpr()) {
    return;
  }

  Unifier.unify(CurrentFun->getReturnTy(), visit(S.getExpr()));
}

void TypeInferencer::visit(DeferStmt &S) { visit(S.getDeferred()); }

void TypeInferencer::visit(ForStmt &S) {
  // 1) Infer the range expression
  auto RangeT = visit(S.getRange());

  // 2) Infer the loop var
  visit(S.getLoopVar());

  // 3) Unify loop var and range
  if (auto Range = llvm::dyn_cast<RangeLiteral>(&S.getRange())) {
    auto Res =
        Unifier.unify(S.getLoopVar().getType(), Range->getStart().getType());
    if (!Res) {
      error("Mismatched types")
          .with_primary_label(S.getLoopVar().getSpan(), "Here")
          .with_secondary_label(S.getRange().getSpan(), "Here")
          .emit(*DiagMan);
    }
  }

  // 4) Infer the loop body
  visit(S.getBody());
}

void TypeInferencer::visit(WhileStmt &S) {
  auto CondT = visit(S.getCond());
  auto Res = Unifier.unify(
      TypeCtx::getBuiltin(BuiltinTy::Bool, S.getCond().getSpan()), CondT);
  if (!Res) {
    error("Condition in while statement is not a bool")
        .with_primary_label(S.getCond().getSpan(),
                            std::format("expected type `bool`, got type {}",
                                        toString(S.getCond().getType())))
        .emit(*DiagMan);
  }

  visit(S.getBody());
}

void TypeInferencer::visit(IfStmt &S) {
  auto CondT = visit(S.getCond());
  auto Res = Unifier.unify(
      TypeCtx::getBuiltin(BuiltinTy::Bool, S.getCond().getSpan()), CondT);
  if (!Res) {
    error("Condition in if statement is not a bool")
        .with_primary_label(
            S.getCond().getSpan(),
            std::format("expected type `bool`, got type {}", toString(CondT)))
        .emit(*DiagMan);
  }

  visit(S.getThen());

  if (S.hasElse()) {
    visit(S.getElse());
  }
}

void TypeInferencer::visit(DeclStmt &S) { visit(S.getDecl()); }
void TypeInferencer::visit(BreakStmt &S) { (void)S; }
void TypeInferencer::visit(ContinueStmt &S) { (void)S; }
void TypeInferencer::visit(ExprStmt &S) { visit(S.getExpr()); }

void TypeInferencer::visit(Block &B) {
  for (const auto &Stmt : B.getStmts()) {
    visit(*Stmt);
  }
}

} // namespace phi
