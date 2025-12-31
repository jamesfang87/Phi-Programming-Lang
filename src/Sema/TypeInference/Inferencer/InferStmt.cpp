#include "Sema/TypeInference/Inferencer.hpp"

#include "AST/TypeSystem/Type.hpp"
#include "Sema/TypeInference/Substitution.hpp"

namespace phi {

TypeInferencer::InferRes TypeInferencer::visit(Stmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(ReturnStmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(DeferStmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(ForStmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(WhileStmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(IfStmt &S) {}
TypeInferencer::InferRes TypeInferencer::visit(DeclStmt &S) {}

TypeInferencer::InferRes TypeInferencer::visit(BreakStmt &S) {
  return {Substitution(),
          TypeCtx::getBuiltin(BuiltinTy::Null, SrcSpan(S.getLocation()))};
}

TypeInferencer::InferRes TypeInferencer::visit(ContinueStmt &S) {
  return {Substitution(),
          TypeCtx::getBuiltin(BuiltinTy::Null, SrcSpan(S.getLocation()))};
}

TypeInferencer::InferRes TypeInferencer::visit(ExprStmt &S) {
  return visit(S.getExpr());
}

TypeInferencer::InferRes TypeInferencer::visit(Block &B) {
  Substitution AllSubsts;
  for (const auto &Stmt : B.getStmts()) {
    auto [StmtSubst, _] = visit(*Stmt);
    AllSubsts.compose(StmtSubst);
  }
  return {AllSubsts,
          TypeCtx::getBuiltin(BuiltinTy::Null,
                              SrcSpan(B.getStmts().back()->getLocation()))};
}

} // namespace phi
