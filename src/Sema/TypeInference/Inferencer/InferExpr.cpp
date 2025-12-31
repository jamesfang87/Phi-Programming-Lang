#include "Sema/TypeInference/Inferencer.hpp"

#include <optional>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/TypeSystem/Context.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Substitution.hpp"

namespace phi {

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {}

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  std::vector<TypeRef> IntConstraints = {
      TypeCtx::getBuiltin(BuiltinTy::i8, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::i16, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::i32, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::i64, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::u8, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::u16, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::u32, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::u64, SrcSpan(E.getLocation())),
  };
  auto T = TypeCtx::getVar(IntConstraints, SrcSpan(E.getLocation()));
  return {Substitution(), T};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  std::vector<TypeRef> FloatConstraints = {
      TypeCtx::getBuiltin(BuiltinTy::f32, SrcSpan(E.getLocation())),
      TypeCtx::getBuiltin(BuiltinTy::f64, SrcSpan(E.getLocation()))};
  auto T = TypeCtx::getVar(FloatConstraints, SrcSpan(E.getLocation()));
  return {Substitution(), T};
}

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = TypeCtx::getBuiltin(BuiltinTy::Bool, SrcSpan(E.getLocation()));
  return {Substitution(), T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = TypeCtx::getBuiltin(BuiltinTy::Char, SrcSpan(E.getLocation()));
  return {Substitution(), T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = TypeCtx::getBuiltin(BuiltinTy::String, SrcSpan(E.getLocation()));
  return {Substitution(), T};
}

TypeInferencer::InferRes TypeInferencer::visit(RangeLiteral &E) {
  auto [StartSubst, StartType] = visit(E.getStart());
  auto [EndSubst, EndType] = visit(E.getEnd());
  Substitution AllSubsts(StartSubst, EndSubst);

  auto Res = unify(AllSubsts.apply(StartType), AllSubsts.apply(EndType));
  if (!Res) {
    return {Substitution(), TypeCtx::getErr(SrcSpan(E.getLocation()))};
  }
  AllSubsts.compose(Res);
  return {AllSubsts,
          TypeCtx::getBuiltin(BuiltinTy::Range, SrcSpan(E.getLocation()))};
}

TypeInferencer::InferRes TypeInferencer::visit(TupleLiteral &E) {
  Substitution AllSubst;
  std::vector<TypeRef> Ts;
  for (auto &Elem : E.getElements()) {
    auto [Subst, T] = visit(*Elem);
    AllSubst.compose(Subst);
    Ts.push_back(AllSubst.apply(T));
  }

  return {AllSubst, TypeCtx::getTuple(Ts, SrcSpan(E.getLocation()))};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  assert(E.getDecl());

  auto Res = unify(E.getDecl()->getType(), E.getType());
  if (!Res) {
    // TODO:
    return {Substitution(), TypeCtx::getErr(SrcSpan(E.getLocation()))};
  }
  E.setType(Res->apply(E.getType()));
  return {*Res, E.getType()};
}

TypeInferencer::InferRes TypeInferencer::visit(FunCallExpr &E) {
  assert(E.getDecl());
  assert(llvm::isa<FunDecl>(E.getDecl()));

  Substitution AllSubst;
  std::vector<TypeRef> Ts;
  for (auto &Arg : E.getArgs()) {
    auto [Subst, T] = visit(*Arg);
    AllSubst.compose(Subst);
    Ts.push_back(Subst.apply(T));
  }
  auto Got = TypeCtx::getFun(
      Ts, TypeCtx::getVar(std::nullopt, SrcSpan(E.getLocation())),
      SrcSpan(E.getLocation()));
  auto Expected = E.getDecl()->getType();

  auto Res = unify(Got, Expected);
  if (!Res) {
    return {Substitution(), TypeCtx::getErr(SrcSpan(E.getLocation()))};
  }
  E.setType(Res->apply(E.getType()));
  return {*Res, E.getType()};
}

TypeInferencer::InferRes TypeInferencer::visit(BinaryOp &E) {}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [AllSubst, OperandType] = visit(E.getOperand());

  switch (E.getOp()) {
  case phi::TokenKind::Bang: {
    auto Bool = TypeCtx::getBuiltin(BuiltinTy::Bool, SrcSpan(E.getLocation()));
    auto Res = unify(Bool, OperandType);
    if (!Res) {
      return {Substitution(), TypeCtx::getErr(SrcSpan(E.getLocation()))};
    }
    AllSubst.compose(*unify(Bool, OperandType));
    E.setType(AllSubst.apply(OperandType));
    return {AllSubst, Bool};
  }
  case phi::TokenKind::Amp: {
    auto T = TypeCtx::getRef(OperandType, SrcSpan(E.getLocation()));
    E.setType(T);
    return {AllSubst, T};
  }
  default:
    E.setType(AllSubst.apply(OperandType));
    return {AllSubst, E.getType()};
  }
}

TypeInferencer::InferRes TypeInferencer::visit(CustomTypeCtor &E) {}
TypeInferencer::InferRes TypeInferencer::visit(MemberInitExpr &E) {}
TypeInferencer::InferRes TypeInferencer::visit(FieldAccessExpr &E) {}
TypeInferencer::InferRes TypeInferencer::visit(MethodCallExpr &E) {}
TypeInferencer::InferRes TypeInferencer::visit(MatchExpr &E) {}

} // namespace phi
