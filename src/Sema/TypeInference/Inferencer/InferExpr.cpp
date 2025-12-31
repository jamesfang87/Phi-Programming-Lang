#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Sema/TypeInference/Inferencer.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "llvm/Support/Casting.h"

namespace phi {

TypeInferencer::InferRes TypeInferencer::visit(Expr &E);
TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E);
TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E);

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = TypeCtx::getBuiltin(BuiltinTy::Bool, SrcSpan(E.getLocation()));
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = TypeCtx::getBuiltin(BuiltinTy::Char, SrcSpan(E.getLocation()));
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E);
TypeInferencer::InferRes TypeInferencer::visit(RangeLiteral &E);
TypeInferencer::InferRes TypeInferencer::visit(TupleLiteral &E);

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  assert(E.getDecl());
}

TypeInferencer::InferRes TypeInferencer::visit(FunCallExpr &E) {
  assert(E.getDecl());
  assert(llvm::isa<FunDecl>(E.getDecl()));
}

TypeInferencer::InferRes TypeInferencer::visit(BinaryOp &E);
TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E);
TypeInferencer::InferRes TypeInferencer::visit(CustomTypeCtor &E);
TypeInferencer::InferRes TypeInferencer::visit(MemberInitExpr &E);
TypeInferencer::InferRes TypeInferencer::visit(FieldAccessExpr &E);
TypeInferencer::InferRes TypeInferencer::visit(MethodCallExpr &E);
TypeInferencer::InferRes TypeInferencer::visit(MatchExpr &E);

} // namespace phi
