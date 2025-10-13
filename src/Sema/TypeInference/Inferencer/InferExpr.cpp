#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeInference/Infer.hpp"

#include <cassert>
#include <optional>
#include <print>
#include <string>
#include <vector>

#include <llvm/Support/Casting.h>

#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Substitution.hpp"

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
  auto T = Monotype::makeCon("bool", {}, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::makeCon("char", {}, E.getLocation());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::makeCon("string", {}, E.getLocation());
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
  auto RangeType = Monotype::makeCon("range", {EndPointType}, E.getLocation());
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

  auto TupleType = Monotype::makeApp("Tuple", Types, E.getLocation());
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
  auto [CalleeSubst, CalleeType] = visit(E.getCallee());
  Substitution AllSubst = CalleeSubst;

  std::vector<Monotype> ArgTypes;
  ArgTypes.reserve(E.getArgs().size());
  for (auto &Arg : E.getArgs()) {
    auto [ArgSubst, ArgType] = visit(*Arg);
    AllSubst.compose(ArgSubst);
    CalleeType = AllSubst.apply(CalleeType);
    ArgTypes.push_back(AllSubst.apply(ArgType));
  }

  const auto RetType = Monotype::makeVar(Factory.fresh());
  const auto ExpectedFunType = Monotype::makeFun(ArgTypes, RetType);
  unifyInto(AllSubst, CalleeType, ExpectedFunType);
  recordSubst(AllSubst);
  annotate(E, AllSubst.apply(RetType));
  return {AllSubst, AllSubst.apply(RetType)};
}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [OperandSubst, OperandType] = visit(E.getOperand());
  Substitution AllSubst = OperandSubst;
  if (E.getOp() == TokenKind::Bang) {
    unifyInto(AllSubst, OperandType, Monotype::makeCon("bool"));
    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon("bool"));
    return {AllSubst, Monotype::makeCon("bool")};
  }

  // Numeric UnaryOp
  auto NewTypeVar = Monotype::makeVar(Factory.fresh());

  auto OpType = Monotype::makeFun({NewTypeVar}, NewTypeVar);
  auto TypeOfCall = Monotype::makeFun({AllSubst.apply(OperandType)},
                                      Monotype::makeVar(Factory.fresh()));
  if (E.getOp() == TokenKind::Amp) {
    OpType =
        Monotype::makeFun({NewTypeVar}, Monotype::makeApp("Ref", {NewTypeVar}));
    assert(OpType.isFun());
    assert(OpType.asFun().Ret->isApp());
    TypeOfCall = Monotype::makeFun(
        {AllSubst.apply(OperandType)},
        Monotype::makeApp("Ref", {AllSubst.apply(OperandType)}));
    assert(TypeOfCall.isFun());
    assert(TypeOfCall.asFun().Ret->isApp());
    unifyInto(AllSubst, OpType, TypeOfCall);
    auto T = TypeOfCall.asFun().Ret;
    recordSubst(AllSubst);
    annotate(E, *T);
    assert(T->isApp());
    return {AllSubst, *T};
  }

  unifyInto(AllSubst, OpType, TypeOfCall);
  recordSubst(AllSubst);
  annotate(E, AllSubst.apply(NewTypeVar));
  return {AllSubst, AllSubst.apply(NewTypeVar)};
}

TypeInferencer::InferRes TypeInferencer::visit(BinaryOp &E) {
  auto [LhsSubst, LhsType] = visit(E.getLhs());
  auto [RhsSubst, RhsType] = visit(E.getRhs());
  Substitution AllSubst = RhsSubst;
  AllSubst.compose(LhsSubst);

  const TokenKind K = E.getOp();

  if (isLogical(K)) {
    unifyInto(AllSubst, LhsType, Monotype::makeCon("bool"));
    unifyInto(AllSubst, RhsType, Monotype::makeCon("bool"));
    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon("bool"));
    return {AllSubst, Monotype::makeCon("bool")};
  }

  if (isComparison(K) || isEquality(K)) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);

    unifyInto(AllSubst, LhsType, RhsType);

    recordSubst(AllSubst);
    annotate(E, Monotype::makeCon("bool"));
    return {AllSubst, Monotype::makeCon("bool")};
  }

  if (isArithmetic(K)) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);
    unifyInto(AllSubst, LhsType, RhsType);

    auto ResultingType = Monotype::makeVar(Factory.fresh());
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
    auto ResultingType = Monotype::makeCon("null");
    annotate(E, ResultingType);
    return {AllSubst, ResultingType};
  }

  throw std::runtime_error("inferBinary: unsupported operator token kind");
}

TypeInferencer::InferRes TypeInferencer::visit(StructLiteral &E) {
  auto Struct = Monotype::makeCon(E.getStructId());
  Substitution AllSubsts;
  for (auto &Field : E.getFields()) {
    auto [FieldSubst, FieldType] = visit(*Field->getValue());
    AllSubsts.compose(FieldSubst);
    if (const auto *FieldDecl = Field->getDecl()) {
      auto DeclaredAs = FieldDecl->getType().toMonotype();
      unifyInto(AllSubsts, DeclaredAs, FieldType);
    }
  }
  recordSubst(AllSubsts);
  annotate(E, Struct);
  return {AllSubsts, Struct};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldInitExpr &E) {
  auto [Subst, Type] = visit(*E.getValue());
  recordSubst(Subst);
  annotate(E, Type);
  return {Subst, Type};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldAccessExpr &E) {
  auto [BaseSubst, BaseType] = visit(*E.getBase());
  auto FieldType = Monotype::makeVar(Factory.fresh());

  std::optional<std::string> TypeName;
  if (BaseType.isApp()) {
    assert(BaseType.isApp() && "BaseType should be Ref<Type>");
    assert(BaseType.asApp().Name == "Ref" &&
           "BaseType should be a ref to a class");
    assert(BaseType.asApp().Args.size() == 1 &&
           "Should hold only 1 Arg (the class)");
    assert(BaseType.asApp().Args.front().isCon() &&
           "The class should be a constant type");

    TypeName = BaseType.asApp().Args.front().asCon().Name;
  } else if (BaseType.isCon()) {
    TypeName = BaseType.asCon().Name;
  }
  assert(TypeName && "Could not infer type before field access");

  const auto It = Structs.find(*TypeName);
  if (It == Structs.end()) {
    auto PrimaryMsg =
        std::format("No declaration for struct `{}` was found.", *TypeName);
    auto Diag = error(std::format("Could not find struct `{}`", *TypeName))
                    .with_primary_label(E.getLocation(), PrimaryMsg);
    Diag.emit(*DiagMan);

    return {BaseSubst, FieldType};
  }
  StructDecl *Struct = It->second;

  const FieldDecl *FieldDecl = Struct->getField(E.getFieldId());
  if (FieldDecl == nullptr) {
    error(
        std::format("attempt to access undeclared field `{}`", E.getFieldId()))
        .with_primary_label(
            E.getLocation(),
            std::format("Declaration for `{}` could not be found in {}.",
                        E.getFieldId(), *TypeName))
        .emit(*DiagMan);
    return {BaseSubst, FieldType};
  }

  // Set the member field for code generation
  E.setMember(const_cast<phi::FieldDecl *>(FieldDecl));

  // Convert the field's AST type to a Monotype and unify with our output
  const auto DeclaredAs = FieldDecl->getType().toMonotype();
  unifyInto(BaseSubst, FieldType, DeclaredAs);

  recordSubst(BaseSubst);
  annotate(E, FieldType);
  return {BaseSubst, FieldType};
}

TypeInferencer::InferRes TypeInferencer::visit(MethodCallExpr &E) {
  // 1) Infer base expression (the receiver)
  auto [BaseSubst, BaseType] = visit(*E.getBase());

  // Ensure base is a struct constructor type
  std::optional<std::string> StructName;
  if (BaseType.isApp()) {
    assert(BaseType.isApp() && "BaseType should be Ref<Type>");
    assert(BaseType.asApp().Name == "Ref" &&
           "BaseType should be a ref to a class");
    assert(BaseType.asApp().Args.size() == 1 &&
           "Should hold only 1 Arg (the class)");
    assert(BaseType.asApp().Args.front().isCon() &&
           "The class should be a constant type");

    StructName = BaseType.asApp().Args.front().asCon().Name;
  } else {
    assert(BaseType.isCon());
    StructName = BaseType.asCon().Name;
  }
  assert(StructName);

  auto It = Structs.find(*StructName);
  if (It == Structs.end()) {
    auto PrimaryMsg =
        std::format("No declaration for struct `{}` was found.", *StructName);
    auto Diag = error(std::format("Could not find struct `{}`", *StructName))
                    .with_primary_label(E.getLocation(), PrimaryMsg);
    Diag.emit(*DiagMan);

    return {BaseSubst, Monotype::makeVar(Factory.fresh(), E.getLocation())};
  }

  StructDecl *Struct = It->second;

  // The callee inside MemberFunCallExpr is expected to be a DeclRefExpr naming
  // the method. If it's not, that's an unsupported form for now.
  auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  if (!DeclRef) {
    throw std::runtime_error(
        "unsupported method call syntax (expected identifier)");
  }

  const std::string MethodName = DeclRef->getId();

  // Find the method declaration inside the struct
  MethodDecl *Method = Struct->getMethod(MethodName);
  E.setDecl(Method);
  E.setMethod(Method);
  if (Method == nullptr) {
    error(std::format("attempt to call undeclared method`{}`", MethodName))
        .with_primary_label(
            E.getLocation(),
            std::format("Declaration for `{}` could not be found in {}.",
                        MethodName, Struct->getId()))
        .emit(*DiagMan);
    return {BaseSubst, Monotype::makeVar(Factory.fresh(), E.getLocation())};
  }

  // 2) Build the method's Monotype from its AST param types and return type
  std::vector<Monotype> MethodParams;
  MethodParams.reserve(Method->getParams().size());
  for (auto &ParamUP : Method->getParams()) {
    auto ParamType = ParamUP->getType().toMonotype();
    MethodParams.push_back(ParamType);
  }

  auto MethodMonotype = E.getMethod().getFunType().toMonotype();
  Substitution S = BaseSubst;

  // receiver arg type (we already have it): use s-applied version
  std::vector<Monotype> CallArgTys;
  CallArgTys.reserve(1 + E.getArgs().size());
  if (llvm::dyn_cast<DeclRefExpr>(E.getBase())->getId() == "this") {
    CallArgTys.push_back(S.apply(BaseType));
  } else {
    CallArgTys.push_back(S.apply(Monotype::makeApp("Ref", {BaseType})));
  }
  for (auto &ArgUP : E.getArgs()) {
    auto [Subst, Type] = visit(*ArgUP);
    S.compose(Subst);
    // make sure to apply the current substitution to argument type
    CallArgTys.push_back(S.apply(Type));
  }

  // 5) Make a fresh result type for the call
  auto ResultTy = Monotype::makeVar(Factory.fresh(), E.getLocation());

  // Expected function shape from the call site: (arg types...) -> ResultTy
  auto FunExpect = Monotype::makeFun(CallArgTys, ResultTy);

  // 6) Unify the declared method monotype with the expected call shape.
  //    This will unify receiver and arguments and produce substitutions.
  unifyInto(S, MethodMonotype, FunExpect);

  // 7) Record the substitution so the environment sees it and annotate the call
  recordSubst(S);
  annotate(E, S.apply(ResultTy));

  return {S, S.apply(ResultTy)};
}

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {
  return E.accept(*this);
}

} // namespace phi
