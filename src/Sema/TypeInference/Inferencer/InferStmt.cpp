#include "Sema/TypeInference/Inferencer.hpp"

#include <cassert>
#include <llvm-18/llvm/ADT/STLExtras.h>
#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>
#include <print>

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

  auto ExprT = visit(S.getExpr());

  std::visit(
      [&](auto Fun) {
        using T = std::decay_t<decltype(Fun)>;

        if constexpr (!std::is_same_v<T, std::monostate>) {
          auto Res = Unifier.unify(Fun->getReturnType(), ExprT);
          if (!Res) {
            error("Mismatched return type")
                .with_primary_label(S.getExpr().getSpan(),
                                    std::format("expected `{}`, got `{}`",
                                                toString(Fun->getReturnType()),
                                                toString(ExprT)))
                .with_secondary_label(
                    Fun->getReturnType().getSpan(),
                    std::format("expected `{}` because of this",
                                toString(Fun->getReturnType())))
                .emit(*Diags);
          }
        }
      },
      CurrentFun);
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
          .emit(*Diags);
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
        .emit(*Diags);
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
        .emit(*Diags);
  }

  visit(S.getThen());

  if (S.hasElse()) {
    visit(S.getElse());
  }
}

void TypeInferencer::visit(DeclStmt &S) {
  assert(S.getDecls().size() > 0);

  if (!S.hasInit()) {
    return;
  }

  visit(S.getInit());
  auto InitT = Unifier.resolve(S.getInit().getType());
  if (S.getDecls().size() > 1 && !InitT.isTuple()) {
    error("cannot destructure non-tuple type")
        .with_primary_label(
            S.getInit().getSpan(),
            std::format("expected this to be a tuple type, not {}",
                        toString(S.getInit().getType())))
        .emit(*Diags);
    return;
  }

  if (S.getDecls().size() > 1) {
    auto Tuple = llvm::dyn_cast<TupleTy>(InitT.getPtr());
    if (Tuple->getElementTys().size() != S.getDecls().size()) {
      error("incorret arity between destructing variables and tuple type")
          .with_primary_label(
              S.getInit().getSpan(),
              std::format(
                  "expected this to be a tuple type of arity {}, not {}",
                  S.getDecls().size(), Tuple->getElementTys().size()))
          .emit(*Diags);
      return;
    }
  }

  for (auto [i, D] : llvm::enumerate(S.getDecls())) {
    auto T = instantiate(D.get());
    auto InitT = S.getInit().getType();
    if (S.getDecls().size() > 1) {
      InitT = llvm::dyn_cast<TupleTy>(InitT.getPtr())->getElementTys()[i];
    }

    auto Res = Unifier.unify(T, InitT);
    if (!Res) {
      error("Mismatched types in variable declaration")
          .with_primary_label(S.getInit().getSpan(),
                              std::format("expected this to be {}, not {}",
                                          toString(D->getType()),
                                          toString(S.getInit().getType())))
          .with_secondary_label(D->getType().getSpan(), "due to this")
          .emit(*Diags);
      return;
    }
  }
}

void TypeInferencer::visit(BreakStmt &S) { (void)S; }
void TypeInferencer::visit(ContinueStmt &S) { (void)S; }
void TypeInferencer::visit(ExprStmt &S) { visit(S.getExpr()); }

void TypeInferencer::visit(Block &B) {
  for (const auto &Stmt : B.getStmts()) {
    visit(*Stmt);
  }
}

} // namespace phi
