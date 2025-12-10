#include "Sema/TypeInference/Infer.hpp"

#include <cassert>
#include <optional>
#include <string>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"

namespace phi {

// ---------------- Expr inference ----------------
// Each inferX returns (Substitution, Monotype). Caller must record/apply
// Substitution.

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  const std::vector<std::string> IntConstraints = {"i8", "i16", "i32", "i64",
                                                   "u8", "u16", "u32", "u64"};
  auto T = Monotype::makeVar(Factory.fresh(), IntConstraints, E.getLocation());
  IntTypeVars.push_back(T.asVar());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  auto T = Monotype::makeVar(Factory.fresh(), {"f32", "f64"}, E.getLocation());
  FloatTypeVars.push_back(T.asVar());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = Monotype::makeCon(PrimitiveKind::Bool, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::makeCon(PrimitiveKind::Char, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::makeCon(PrimitiveKind::String, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(RangeLiteral &E) {
  auto [StartSubst, StartType] = visit(E.getStart());
  auto [EndSubst, EndType] = visit(E.getEnd());
  Substitution AllSubsts = EndSubst;
  AllSubsts.compose(StartSubst);

  unifyInto(AllSubsts, StartType, EndType);
  recordSubst(AllSubsts);
  auto EndPointType = AllSubsts.apply(StartType);
  auto RangeType = Monotype::makeApp(TypeApp::BuiltinKind::Range,
                                     {EndPointType}, E.getLocation());
  annotate(E, RangeType);
  return {AllSubsts, RangeType};
}

TypeInferencer::InferRes TypeInferencer::visit(TupleLiteral &E) {
  Substitution AllSubsts;
  std::vector<Monotype> Types;
  for (auto &Element : E.getElements()) {
    auto [ElementSubst, ElementType] = visit(*Element);
    Types.push_back(ElementType);
    AllSubsts.compose(ElementSubst);
  }

  auto TupleType =
      Monotype::makeApp(TypeApp::BuiltinKind::Tuple, Types, E.getLocation());
  annotate(E, TupleType);
  return {AllSubsts, TupleType};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  std::optional<Polytype> DeclaredAs;

  if (const auto *Decl = E.getDecl()) {
    DeclaredAs = Env.lookup(Decl);
  } else {
    DeclaredAs = Env.lookup(E.getId());
  }
  assert(DeclaredAs);

  Monotype T = DeclaredAs->instantiate(Factory);
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(FunCallExpr &E) {
  Substitution AllSubst;

  std::vector<Monotype> ArgTypes;
  ArgTypes.reserve(E.getArgs().size());
  for (auto &Arg : E.getArgs()) {
    auto [ArgSubst, ArgType] = visit(*Arg);
    AllSubst.compose(ArgSubst);
    ArgTypes.push_back(AllSubst.apply(ArgType));
  }

  const auto RetType = E.getDecl()->getReturnTy().toMonotype();
  const auto DeclaredType = E.getDecl()->getFunType().toMonotype();
  const auto GotType = Monotype::makeFun(ArgTypes, RetType);
  unifyInto(AllSubst, DeclaredType, GotType);
  recordSubst(AllSubst);
  annotate(E, RetType);
  return {AllSubst, RetType};
}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [OperandSubst, OperandType] = visit(E.getOperand());
  Substitution AllSubst = OperandSubst;

  if (E.getOp() == TokenKind::Bang) {
    auto Bool = Monotype::makeCon(PrimitiveKind::Bool, E.getLocation());
    unifyInto(AllSubst, OperandType, Bool);
    recordSubst(AllSubst);
    annotate(E, Bool);
    return {AllSubst, Bool};
  }

  // Apply current substitutions so we annotate with a resolved type.
  OperandType = AllSubst.apply(OperandType);

  if (E.getOp() == TokenKind::Amp) {
    // &: result is Ref<operand-type>
    auto Result = Monotype::makeApp(TypeApp::BuiltinKind::Ref, {OperandType});
    recordSubst(AllSubst);
    annotate(E, Result);
    return {AllSubst, Result};
  }

  // Numeric unary: result is operand's type (after applying substitutions)
  recordSubst(AllSubst);
  annotate(E, OperandType);
  return {AllSubst, OperandType};
}

TypeInferencer::InferRes TypeInferencer::visit(BinaryOp &E) {
  auto [LhsSubst, LhsType] = visit(E.getLhs());
  auto [RhsSubst, RhsType] = visit(E.getRhs());
  Substitution AllSubst = RhsSubst;
  AllSubst.compose(LhsSubst);

  const TokenKind K = E.getOp();

  if (isLogical(K)) {
    unifyInto(AllSubst, LhsType,
              Monotype::makeCon(PrimitiveKind::Bool, LhsType.getLocation()));
    unifyInto(AllSubst, RhsType,
              Monotype::makeCon(PrimitiveKind::Bool, RhsType.getLocation()));
    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon(PrimitiveKind::Bool, E.getLocation()));
    return {AllSubst, Monotype::makeCon(PrimitiveKind::Bool, E.getLocation())};
  }

  if (isComparison(K) || isEquality(K)) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);

    unifyInto(AllSubst, LhsType, RhsType);

    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon(PrimitiveKind::Bool, E.getLocation()));
    return {AllSubst, Monotype::makeCon(PrimitiveKind::Bool, E.getLocation())};
  }

  if (isArithmetic(K)) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);
    unifyInto(AllSubst, LhsType, RhsType);

    // New type created for better location info
    auto ResultingType = Monotype::makeVar(Factory.fresh(), E.getLocation());
    unifyInto(AllSubst, LhsType, ResultingType);
    recordSubst(AllSubst);
    annotate(E, ResultingType);
    return {AllSubst, ResultingType};
  }

  if (K == TokenKind::Equals) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);

    unifyInto(AllSubst, LhsType, RhsType);
    recordSubst(AllSubst);

    // Assignment expressions evaluate to null
    auto ResultType = Monotype::makeCon(PrimitiveKind::Null, E.getLocation());
    annotate(E, ResultType);
    return {AllSubst, ResultType};
  }

  throw std::runtime_error("inferBinary: unsupported operator token kind");
}

TypeInferencer::InferRes TypeInferencer::visit(CustomTypeCtor &E) {
  if (E.isAnonymous()) {
    return {Substitution{},
            Monotype::makeVar(Factory.fresh(), E.getLocation())};
  }

  Monotype Type;
  switch (E.getInterpretation()) {
  case CustomTypeCtor::InterpretAs::Struct:
    Type = Monotype::makeCon(E.getTypeName(), TypeCon::Struct, E.getLocation());
    break;
  case CustomTypeCtor::InterpretAs::Enum:
    Type = Monotype::makeCon(E.getTypeName(), TypeCon::Enum, E.getLocation());
    break;
  case CustomTypeCtor::InterpretAs::Unknown:
    static_assert("A CustomTypeCtor which is not anonymous should not have an "
                  "interpretation of `Unknown`");
  }

  Substitution AllSubsts;
  for (auto &Init : E.getInits()) {
    auto [InitSubst, InitType] = visit(*Init->getInitValue());
    AllSubsts.compose(InitSubst);

    if (const auto *Decl = Init->getDecl()) {
      auto DeclaredType = Decl->getType().toMonotype();
      unifyInto(AllSubsts, DeclaredType, InitType);
    }
  }

  recordSubst(AllSubsts);
  annotate(E, Type);
  return {AllSubsts, Type};
}

TypeInferencer::InferRes TypeInferencer::visit(MemberInitExpr &E) {
  auto [Subst, Type] = visit(*E.getInitValue());
  recordSubst(Subst);
  annotate(E, Type);
  return {Subst, Type};
}

TypeInferencer::InferRes TypeInferencer::visit(MatchExpr &E) {
  auto [AllSubst, ScrutineeType] = visit(*E.getScrutinee());

  Monotype ReturnType;
  for (const MatchExpr::Arm &Arm : E.getArms()) {
    // be my, be my love

    auto [BodySubst, _] = visit(*Arm.Body);
    AllSubst.compose(BodySubst);

    // does visiting twice do anything bad?

    auto [_, ArmType] = visit(*Arm.Return);
    unify(ReturnType, ArmType); // does this
  }

  // Since MatchExpr returns a type, we must check
  // whether all arms return the same type here rather
  // than in type checking

  return {AllSubst, ReturnType};
}

std::tuple<Substitution, Monotype, StructDecl *>
TypeInferencer::inferStructBase(Expr &BaseExpr, const SrcLocation &Loc) {
  auto [BaseSubst, BaseType] = visit(BaseExpr);

  auto StructName = BaseType.toAstType().unwrap().getCustomName();
  assert(StructName && "Could not infer type before struct access");

  auto It = Structs.find(*StructName);
  if (It == Structs.end()) {
    auto PrimaryMsg =
        std::format("No declaration for struct `{}` was found.", *StructName);
    error(std::format("Could not find struct `{}`", *StructName))
        .with_primary_label(Loc, PrimaryMsg)
        .emit(*DiagMan);

    return {BaseSubst, BaseType, nullptr};
  }

  return {BaseSubst, BaseType, It->second};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldAccessExpr &E) {
  auto [BaseSubst, BaseType, Struct] =
      inferStructBase(*E.getBase(), E.getLocation());
  auto FieldType = Monotype::makeVar(Factory.fresh());

  if (!Struct) {
    return {BaseSubst, FieldType};
  }

  FieldDecl *FieldDecl = Struct->getField(E.getFieldId());
  if (!FieldDecl) {
    error(
        std::format("attempt to access undeclared field `{}`", E.getFieldId()))
        .with_primary_label(
            E.getLocation(),
            std::format("Declaration for `{}` could not be found in {}.",
                        E.getFieldId(), Struct->getId()))
        .emit(*DiagMan);
    return {BaseSubst, FieldType};
  }
  E.setMember(FieldDecl);

  return unifyAndAnnotate(E, BaseSubst, FieldDecl->getType().toMonotype(),
                          FieldType);
}

TypeInferencer::InferRes TypeInferencer::visit(MethodCallExpr &E) {
  auto [BaseSubst, BaseType, Struct] =
      inferStructBase(*E.getBase(), E.getLocation());
  if (!Struct) {
    auto Ret = Monotype::makeVar(Factory.fresh(), E.getLocation());
    return {BaseSubst, Ret};
  }

  auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  assert(DeclRef && "unsupported method call syntax");

  const std::string MethodName = DeclRef->getId();
  MethodDecl *Method = Struct->getMethod(MethodName);
  E.setMethod(Method);
  if (!Method) {
    error(std::format("attempt to call undeclared method `{}`", MethodName))
        .with_primary_label(
            E.getLocation(),
            std::format("Declaration for `{}` could not be found in {}.",
                        MethodName, Struct->getId()))
        .emit(*DiagMan);
    return {BaseSubst, Monotype::makeVar(Factory.fresh(), E.getLocation())};
  }

  auto DeclaredType = Method->getFunType().toMonotype();
  Substitution AllSubst = BaseSubst;

  std::vector<Monotype> ArgTypes = {AllSubst.apply(
      llvm::dyn_cast<DeclRefExpr>(E.getBase())->getId() == "this"
          ? BaseType
          : Monotype::makeApp(TypeApp::BuiltinKind::Ref, {BaseType}))};
  ArgTypes.reserve(1 + E.getArgs().size());
  for (auto &Arg : E.getArgs()) {
    auto [Subst, Type] = visit(*Arg);
    AllSubst.compose(Subst);
    ArgTypes.push_back(AllSubst.apply(Type));
  }

  auto RetType = Method->getReturnTy().toMonotype();
  auto GotType = Monotype::makeFun(ArgTypes, RetType);
  unifyInto(AllSubst, GotType, DeclaredType);
  recordSubst(AllSubst);
  annotate(E, AllSubst.apply(RetType));
  return {AllSubst, AllSubst.apply(RetType)};
}

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {
  return E.accept(*this);
}

} // namespace phi
