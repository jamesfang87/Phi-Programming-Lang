#include "Sema/TypeInference/Inferencer.hpp"

#include <llvm/ADT/TypeSwitch.h>

namespace phi {

void TypeInferencer::finalize(Stmt &S) {
  llvm::TypeSwitch<Stmt *>(&S)
      .Case<ReturnStmt>([&](ReturnStmt *X) { finalize(*X); })
      .Case<DeferStmt>([&](DeferStmt *X) { finalize(*X); })
      .Case<IfStmt>([&](IfStmt *X) { finalize(*X); })
      .Case<WhileStmt>([&](WhileStmt *X) { finalize(*X); })
      .Case<ForStmt>([&](ForStmt *X) { finalize(*X); })
      .Case<DeclStmt>([&](DeclStmt *X) { finalize(*X); })
      .Case<ContinueStmt>([&](ContinueStmt *X) { finalize(*X); })
      .Case<BreakStmt>([&](BreakStmt *X) { finalize(*X); })
      .Case<ExprStmt>([&](ExprStmt *X) { finalize(*X); })
      .Default([&](Stmt *) { std::unreachable(); });
}

void TypeInferencer::finalize(ReturnStmt &S) { finalize(S.getExpr()); }
void TypeInferencer::finalize(DeferStmt &S) { finalize(S.getDeferred()); }

void TypeInferencer::finalize(ForStmt &S) {
  finalize(S.getRange());

  auto &D = S.getLoopVar();
  D.setType(Unifier.resolve(D.getType()));
  if (D.getType().isVar()) {
    auto Int = llvm::dyn_cast<VarTy>(D.getType().getPtr());
    assert(Int->getDomain() == VarTy::Int);
    D.setType(TypeCtx::getBuiltin(BuiltinTy::i32, D.getSpan()));
  }

  finalize(S.getBody());
}

void TypeInferencer::finalize(WhileStmt &S) {
  finalize(S.getCond());
  finalize(S.getBody());
}

void TypeInferencer::finalize(IfStmt &S) {
  finalize(S.getCond());
  finalize(S.getThen());
  if (S.hasElse())
    finalize(S.getElse());
}

void TypeInferencer::finalize(DeclStmt &S) { finalize(S.getDecl()); }
void TypeInferencer::finalize(BreakStmt &S) { (void)S; }
void TypeInferencer::finalize(ContinueStmt &S) { (void)S; }
void TypeInferencer::finalize(ExprStmt &S) { finalize(S.getExpr()); }

void TypeInferencer::finalize(Block &B) {
  for (auto &S : B.getStmts()) {
    finalize(*S);
  }
}

} // namespace phi
