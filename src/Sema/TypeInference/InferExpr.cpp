#include "Lexer/TokenKind.hpp"
#include "Sema/TypeInference/Algorithms.hpp"
#include "Sema/TypeInference/Infer.hpp"
#include <cassert>
#include <llvm/Support/Casting.h>

#include <print>
#include <string>
#include <vector>

namespace phi {

// ---------------- Expr inference ----------------
// Each inferX returns (Substitution, Monotype). Caller must record/apply
// Substitution.

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  const std::vector<std::string> IntConstraints = {"i8", "i16", "i32", "i64",
                                                   "u8", "u16", "u32", "u64"};
  auto T = Monotype::makeVar(Factory.fresh(), IntConstraints);
  IntTypeVars.push_back(T.asVar());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  auto T = Monotype::makeVar(Factory.fresh(), {"f32", "f64"});
  FloatTypeVars.push_back(T.asVar());
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = Monotype::makeCon("bool");
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::makeCon("char");
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::makeCon("string");
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
  auto RangeType = Monotype::makeCon("range", {EndPointType});
  annotate(E, RangeType);
  return {AllSubsts, RangeType};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  std::optional<Polytype> DeclaredAs;

  if (const auto *Decl = E.getDecl()) {
    DeclaredAs = Env.lookup(Decl);
  } else {
    // fallback by name (could be a function or var)
    DeclaredAs = Env.lookup(E.getId());
  }

  assert(DeclaredAs);
  Monotype T = instantiate(*DeclaredAs, Factory);
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
  if (E.getOp() == TokenKind::BangKind) {
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
  if (E.getOp() == TokenKind::AmpKind) {
    std::println("here for amp");
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
    std::println("OpType: {} | TypeOfCall: {}", OpType.toString(),
                 TypeOfCall.toString());
    recordSubst(AllSubst);
    annotate(E, *T);
    assert(T->isApp());
    return {AllSubst, *T};
  }

  unifyInto(AllSubst, OpType, TypeOfCall);
  std::println("OpType: {} | TypeOfCall: {}", OpType.toString(),
               TypeOfCall.toString());
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

    // Standard arithmetic unification for non-float cases
    auto NewTypeVar = Monotype::makeVar(Factory.fresh());
    const auto OpType = Monotype::makeFun({NewTypeVar, NewTypeVar}, NewTypeVar);
    const auto TypeOfCall = Monotype::makeFun(
        {LhsType, RhsType}, Monotype::makeVar(Factory.fresh()));
    unifyInto(AllSubst, OpType, TypeOfCall);
    recordSubst(AllSubst);
    auto ResultingType = AllSubst.apply(NewTypeVar);
    annotate(E, ResultingType);
    return {AllSubst, ResultingType};
  }

  if (K == TokenKind::EqualsKind) {
    LhsType = AllSubst.apply(LhsType);
    RhsType = AllSubst.apply(RhsType);

    unifyInto(AllSubst, LhsType, RhsType);
    recordSubst(AllSubst);

    // Assignment expressions evaluate to unit
    auto ResultingType = Monotype::makeCon("unit");
    annotate(E, ResultingType);
    return {AllSubst, ResultingType};
  }

  throw std::runtime_error("inferBinary: unsupported operator token kind");
}

TypeInferencer::InferRes TypeInferencer::visit(StructInitExpr &E) {
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

TypeInferencer::InferRes TypeInferencer::visit(MemberAccessExpr &E) {
  auto [BaseSubst, BaseType] = visit(*E.getBase());
  auto FieldType = Monotype::makeVar(Factory.fresh());

  auto [TypeName, Args] = BaseType.asCon();

  const auto It = Structs.find(TypeName);
  if (It == Structs.end()) {
    std::println("Could not find struct {} in symbol table", TypeName);
    return {BaseSubst, FieldType};
  }
  StructDecl *Struct = It->second;

  const FieldDecl *FieldDecl = Struct->getField(E.getMemberId());
  if (FieldDecl == nullptr) {
    std::println("Could not find field {} in struct {}", E.getMemberId(),
                 TypeName);
    return {BaseSubst, FieldType};
  }

  // Convert the field's AST type to a Monotype and unify with our output
  const auto DeclaredAs = FieldDecl->getType().toMonotype();
  unifyInto(BaseSubst, FieldType, DeclaredAs);

  recordSubst(BaseSubst);
  annotate(E, FieldType);
  return {BaseSubst, FieldType};
}

TypeInferencer::InferRes TypeInferencer::visit(MemberFunCallExpr &E) {
  // 1) Infer base expression (the receiver)
  auto [sBase, tBase] = visit(*E.getBase());

  // Ensure base is a struct constructor type
  if (!tBase.isCon()) {
    throw std::runtime_error("method call on non-struct type: " +
                             tBase.toString());
  }

  const std::string StructName = tBase.asCon().Name;

  // Lookup the struct in your symbol table
  auto it = Structs.find(StructName);
  if (it == Structs.end()) {
    throw std::runtime_error("unknown struct type: " + StructName);
  }
  StructDecl *Struct = it->second;

  // The callee inside MemberFunCallExpr is expected to be a DeclRefExpr naming
  // the method. If it's not, that's an unsupported form for now.
  auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCall().getCallee());
  if (!DeclRef) {
    throw std::runtime_error(
        "unsupported method call syntax (expected identifier)");
  }

  const std::string MethodName = DeclRef->getId();

  // Find the method declaration inside the struct
  FunDecl *Method = Struct->getMethod(MethodName);
  E.getCall().setDecl(Method);
  if (!Method) {
    throw std::runtime_error("struct '" + StructName + "' has no method '" +
                             MethodName + "'");
  }

  // 2) Build the method's Monotype from its AST param types and return type
  std::vector<Monotype> MethodParams;
  MethodParams.reserve(Method->getParams().size());

  for (auto &ParamUP : Method->getParams()) {
    ParamDecl *P = ParamUP.get();
    if (!P->hasType()) {
      throw std::runtime_error("method parameter '" + P->getId() +
                               "' missing type annotation");
    }
    auto PTy = P->getType().toMonotype();
    MethodParams.push_back(PTy);
  }

  // Return type must be present (your FunDecl stores return type)
  auto RetTy = Method->getReturnTy().toMonotype();
  // 3) Prepend the receiver type to the parameter list to form full function
  // type
  //    Here the receiver type is tBase (the concrete struct monotype).
  std::vector<Monotype> FullParams;
  FullParams.reserve(1 + MethodParams.size());
  FullParams.push_back(
      sBase.apply(tBase)); // apply substitution so receiver is up-to-date
  for (auto &p : MethodParams)
    FullParams.push_back(p);

  auto MethodMonotype = Monotype::makeFun(std::move(FullParams), RetTy);
  // 4) Now infer the call: first collect argument types (receiver + explicit
  // args) Start with the base substitution sBase
  Substitution S = sBase;

  // receiver arg type (we already have it): use s-applied version
  std::vector<Monotype> CallArgTys;
  CallArgTys.reserve(1 + E.getCall().getArgs().size());
  CallArgTys.push_back(S.apply(tBase)); // receiver goes first

  // Infer each explicit argument, composing substitutions as we go
  for (auto &ArgUP : E.getCall().getArgs()) {
    auto [si, ti] = visit(*ArgUP);
    S.compose(si);
    // make sure to apply the current substitution to argument type
    CallArgTys.push_back(S.apply(ti));
  }

  // 5) Make a fresh result type for the call
  auto ResultTy = Monotype::makeVar(Factory.fresh());

  // Expected function shape from the call site: (arg types...) -> ResultTy
  auto FnExpect = Monotype::makeFun(CallArgTys, ResultTy);

  // 6) Unify the declared method monotype with the expected call shape.
  //    This will unify receiver and arguments and produce substitutions.
  unifyInto(S, MethodMonotype, FnExpect);

  // 7) Record the substitution so the environment sees it and annotate the call
  recordSubst(S);
  annotate(E, S.apply(ResultTy));

  return {S, S.apply(ResultTy)};
}

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {
  return E.accept(*this);
}

} // namespace phi
