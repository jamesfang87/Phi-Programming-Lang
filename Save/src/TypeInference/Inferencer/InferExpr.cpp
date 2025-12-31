#include "Sema/TypeInference/Infer.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>
#include <variant>
#include <vector>

#include <llvm/Support/Casting.h>

#include "AST/Nodes/Expr.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"
#include "llvm/IR/DebugProgramInstruction.h"

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
  auto T = Monotype::makeCon(BuiltinKind::Bool, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::makeCon(BuiltinKind::Char, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::makeCon(BuiltinKind::String, E.getLocation());
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
    auto Bool = Monotype::makeCon(BuiltinKind::Bool, E.getLocation());
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
              Monotype::makeCon(BuiltinKind::Bool, LhsType.getLocation()));
    unifyInto(AllSubst, RhsType,
              Monotype::makeCon(BuiltinKind::Bool, RhsType.getLocation()));
    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon(BuiltinKind::Bool, E.getLocation()));
    return {AllSubst, Monotype::makeCon(BuiltinKind::Bool, E.getLocation())};
  }

  if (isComparison(K) || isEquality(K)) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);

    unifyInto(AllSubst, LhsType, RhsType);

    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon(BuiltinKind::Bool, E.getLocation()));
    return {AllSubst, Monotype::makeCon(BuiltinKind::Bool, E.getLocation())};
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
    auto ResultType = Monotype::makeCon(BuiltinKind::Null, E.getLocation());
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
    Type = Monotype::makeCon(E.getTypeName(), E.getAsStruct(), E.getLocation());
    break;
  case CustomTypeCtor::InterpretAs::Enum:
    Type = Monotype::makeCon(E.getTypeName(), E.getAsEnum(), E.getLocation());
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

  auto visitPattern = [&](const Pattern &Pat) -> InferRes {
    return std::visit(
        [&](auto const &Pattern) -> InferRes {
          using T = std::decay_t<decltype(Pattern)>;

          if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
            assert(Pattern.Value && "Literal pattern has no expression value");
            auto [LiteralSubst, LiteralType] = visit(*Pattern.Value);
            LiteralType = LiteralSubst.apply(LiteralType);
            return {LiteralSubst, LiteralType};
          }

          else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
            for (auto &Var : Pattern.Vars) {
              visit(*Var);
            }

            SrcLocation Loc;
            return {Substitution{}, Monotype::makeVar(Factory.fresh(), Loc)};
          }

          else {
            SrcLocation Loc;
            return {Substitution{}, Monotype::makeVar(Factory.fresh(), Loc)};
          }
        },
        Pat);
  };

  Monotype ReturnType = Monotype::makeVar(Factory.fresh(), E.getLocation());
  for (const MatchExpr::Arm &Arm : E.getArms()) {
    for (auto &Pattern : Arm.Patterns) {
      auto [PatternSubst, PatternType] = visitPattern(Pattern);
      AllSubst.compose(PatternSubst);

      ScrutineeType = AllSubst.apply(ScrutineeType);
      PatternType = AllSubst.apply(PatternType);
      unifyInto(AllSubst, ScrutineeType, PatternType);
      recordSubst(AllSubst);
    }

    auto [BodySubst, _] = visit(*Arm.Body);
    AllSubst.compose(BodySubst);

    // does visiting twice do anything bad?
    auto [_, ArmType] = visit(*Arm.Return);
    unifyInto(AllSubst, ReturnType, ArmType);
  }

  annotate(E, ReturnType);
  return {AllSubst, ReturnType};
}

std::tuple<Substitution, Monotype, AdtDecl *>
TypeInferencer::inferStructBase(Expr &BaseExpr) {
  auto [BaseSubst, BaseType] = visit(BaseExpr);
  auto AstType = BaseType.toAstType();

  auto Name = AstType.unwrap().getCustomName();
  assert(Name && "Could not infer type before struct access");

  if (AstType.unwrap().isStruct()) {
    auto StructsIt = Structs.find(*Name);
    if (StructsIt != Structs.end()) {
      return {BaseSubst, BaseType, StructsIt->second};
    }
  }

  if (AstType.unwrap().isEnum()) {
    auto EnumsIt = Enums.find(*Name);
    if (EnumsIt != Enums.end()) {
      return {BaseSubst, BaseType, EnumsIt->second};
    }
  }

  auto Msg = std::format("No declaration for `{}` was found.", *Name);
  error(std::format("Could not find `{}`", *Name))
      .with_primary_label(BaseExpr.getLocation(), Msg)
      .emit(*DiagMan);

  return {BaseSubst, BaseType, nullptr};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldAccessExpr &E) {
  auto [BaseSubst, BaseType, StructPtr] = inferStructBase(*E.getBase());
  auto FieldType = Monotype::makeVar(Factory.fresh());

  if (!StructPtr) {
    return {BaseSubst, FieldType};
  }

  assert(StructPtr && "nullptr StructPtr on field access");
  assert(llvm::isa<StructDecl>(StructPtr) && "Field access on enum");
  FieldDecl *Field =
      llvm::dyn_cast<StructDecl>(StructPtr)->getField(E.getFieldId());
  if (Field) {
    std::println("here");
    E.setField(Field);
    assert(E.getField());
    return unifyAndAnnotate(E, BaseSubst, Field->getType().toMonotype(),
                            FieldType);
  }

  error(std::format("attempt to access undeclared field `{}`", E.getFieldId()))
      .with_primary_label(
          E.getLocation(),
          std::format("Declaration for `{}` could not be found in {}.",
                      E.getFieldId(), StructPtr->getId()))
      .emit(*DiagMan);
  return {BaseSubst, FieldType};
}

TypeInferencer::InferRes TypeInferencer::visit(MethodCallExpr &E) {
  auto [BaseSubst, BaseType, ADT] = inferStructBase(*E.getBase());
  if (!ADT) {
    auto Ret = Monotype::makeVar(Factory.fresh(), E.getLocation());
    return {BaseSubst, Ret};
  }

  auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  assert(DeclRef && "unsupported method call syntax");

  const std::string MethodName = DeclRef->getId();
  MethodDecl *Method = ADT->getMethod(MethodName);
  E.setMethod(Method);
  if (!Method) {
    error(std::format("attempt to call undeclared method `{}`", MethodName))
        .with_primary_label(
            E.getLocation(),
            std::format("Declaration for `{}` could not be found in {}.",
                        MethodName, ADT->getId()))
        .emit(*DiagMan);
    return {BaseSubst, Monotype::makeVar(Factory.fresh(), E.getLocation())};
  }

  auto DeclaredType = Method->getType().toMonotype();
  Substitution AllSubst = BaseSubst;

  std::vector<Monotype> ArgTypes = {Monotype::makeVar(Factory.fresh())};
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
  return {AllSubst, RetType};
}

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {
  return E.accept(*this);
}

} // namespace phi
