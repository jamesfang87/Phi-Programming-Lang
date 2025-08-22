#include "Sema/HMTI/Algorithms.hpp"
#include "Sema/HMTI/Infer.hpp"
#include <llvm/Support/Casting.h>

#include <print>
#include <string>
#include <vector>

namespace phi {

// ---------------- Expr inference ----------------
// Each inferX returns (Substitution, Monotype). Caller must record/apply
// Substitution.

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  std::vector<std::string> IntConstraints = {"i8", "i16", "i32", "i64",
                                             "u8", "u16", "u32", "u64"};
  auto Tv = Monotype::makeVar(Factory.fresh());
  Tv.asVar().Constraints = IntConstraints;
  IntTypeVars.push_back(Tv.asVar());
  annotate(E, Tv);
  return {Substitution{}, Tv};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  auto Tv = Monotype::makeVar(Factory.fresh());
  // Add float type constraints
  Tv.asVar().Constraints = {"f32", "f64"};
  FloatTypeVars.push_back(Tv.asVar());
  annotate(E, Tv);
  return {Substitution{}, Tv};
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
  auto [s1, tStart] = visit(E.getStart());
  auto [s2, tEnd] = visit(E.getEnd());
  Substitution S = s2;
  S.compose(s1);
  unifyInto(S, tStart, tEnd);
  recordSubst(S);
  auto TT = S.apply(tStart);
  auto RangeT = Monotype::makeCon("range", {TT});
  annotate(E, RangeT);
  return {S, RangeT};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  if (auto *VD = E.getDecl()) {
    auto Sc = Env.lookup(VD);
    if (!Sc)
      throw std::runtime_error("unbound declaration: " + E.getId());
    auto T = instantiate(*Sc, Factory);
    annotate(E, T);
    return {Substitution{}, T};
  }
  // fallback by name (could be a function or var)
  auto Scn = Env.lookup(E.getId());
  if (!Scn)
    throw std::runtime_error("unbound identifier: " + E.getId());
  auto T = instantiate(*Scn, Factory);
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(FunCallExpr &E) {
  auto [sC, tCallee] = visit(E.getCallee());
  Substitution s = sC;
  std::vector<Monotype> ArgTys;
  ArgTys.reserve(E.getArgs().size());
  for (auto &ArgUP : E.getArgs()) {
    auto [si, ti] = visit(*ArgUP);
    s.compose(si);
    tCallee = s.apply(tCallee);
    ArgTys.push_back(s.apply(ti));
  }
  auto tRes = Monotype::makeVar(Factory.fresh());
  auto fnExpect = Monotype::makeFun(ArgTys, tRes);
  unifyInto(s, tCallee, fnExpect);
  recordSubst(s);
  annotate(E, s.apply(tRes));
  return {s, s.apply(tRes)};
}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [s1, tO] = visit(E.getOperand());
  Substitution s = s1;
  if (E.getOp() == TokenKind::BangKind) {
    unifyInto(s, tO, Monotype::makeCon("bool"));
    recordSubst(s);
    annotate(E, Monotype::makeCon("bool"));
    return {s, Monotype::makeCon("bool")};
  }
  // numeric unary -
  auto a = Monotype::makeVar(Factory.fresh());
  auto opTy = Monotype::makeFun({a}, a);
  auto callTy =
      Monotype::makeFun({s.apply(tO)}, Monotype::makeVar(Factory.fresh()));
  unifyInto(s, opTy, callTy);
  recordSubst(s);
  annotate(E, s.apply(a));
  return {s, s.apply(a)};
}

TypeInferencer::InferRes TypeInferencer::visit(BinaryOp &E) {
  auto [sL, tL] = visit(E.getLhs());
  auto [sR, tR] = visit(E.getRhs());
  Substitution s = sR;
  s.compose(sL);

  TokenKind K = E.getOp();

  if (isLogical(K)) {
    unifyInto(s, tL, Monotype::makeCon("bool"));
    unifyInto(s, tR, Monotype::makeCon("bool"));
    recordSubst(s);
    annotate(E, Monotype::makeCon("bool"));
    return {s, Monotype::makeCon("bool")};
  }

  if (isComparison(K) || isEquality(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    unifyInto(s, tl, tr);

    recordSubst(s);
    annotate(E, Monotype::makeCon("bool"));
    return {s, Monotype::makeCon("bool")};
  }

  if (isArithmetic(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // Standard arithmetic unification for non-float cases
    auto a = Monotype::makeVar(Factory.fresh());
    auto opTy = Monotype::makeFun({a, a}, a);
    auto callTy =
        Monotype::makeFun({tl, tr}, Monotype::makeVar(Factory.fresh()));
    unifyInto(s, opTy, callTy);
    recordSubst(s);
    auto res = s.apply(a);
    annotate(E, res);
    return {s, res};
  }

  if (K == TokenKind::EqualsKind) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // In Rust: lhs and rhs must have the same type
    unifyInto(s, tl, tr);

    recordSubst(s);

    // Assignment expressions evaluate to unit
    auto res = Monotype::makeCon("unit");
    annotate(E, res);
    return {s, res};
  }

  throw std::runtime_error("inferBinary: unsupported operator token kind");
}
TypeInferencer::InferRes TypeInferencer::visit(StructInitExpr &E) {
  auto st = Monotype::makeCon(E.getStructId());
  Substitution s;
  for (auto &f : E.getFields()) {
    auto [si, tv] = visit(*f->getValue());
    s.compose(si);
    if (auto *FD = f->getDecl()) {
      auto fieldTy = FD->getType().toMonotype();
      unifyInto(s, fieldTy, tv);
    }
  }
  recordSubst(s);
  annotate(E, st);
  return {s, st};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldInitExpr &E) {
  auto [s, tv] = visit(*E.getValue());
  recordSubst(s);
  annotate(E, tv);
  return {s, tv};
}

TypeInferencer::InferRes TypeInferencer::visit(MemberAccessExpr &E) {
  auto [s1, BaseType] = visit(*E.getBase());
  auto out = Monotype::makeVar(Factory.fresh());

  TypeCon StructType = BaseType.asCon();

  auto it = Structs.find(StructType.Name);
  if (it == Structs.end()) {
    std::println("Could not find struct {} in symbol table", StructType.Name);
    return {s1, out};
  }
  StructDecl *Struct = it->second;

  FieldDecl *Field = Struct->getField(E.getMemberId());
  if (Field == nullptr) {
    std::println("Could not find field {} in struct {}", E.getMemberId(),
                 StructType.Name);
    return {s1, out};
  }

  // Convert the field's AST type to a Monotype and unify with our output
  auto fieldType = Field->getType().toMonotype();
  unifyInto(s1, out, fieldType);

  recordSubst(s1);
  annotate(E, out);
  return {s1, out};
}

// Replace your existing MemberFunCallExpr handler with this:

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
