#include "Sema/HMTI/Infer.hpp"

#include <llvm/Support/Casting.h>

#include <print>

namespace phi {

// ---------------- Expr inference ----------------
// Each inferX returns (Substitution, Monotype). Caller must record/apply
// Substitution.

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  // create fresh type variable for integer literal (polymorphic int)
  auto Tv = Monotype::var(Factory.fresh());
  IntLiteralVars.push_back(Tv->asVar());
  // store monotype in side table (do not set AST::Type yet)
  annotate(E, Tv);
  return {Substitution{}, Tv};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  // create fresh type variable for integer literal (polymorphic int)
  auto Tv = Monotype::var(Factory.fresh());
  FloatLiteralVars.push_back(Tv->asVar());
  // store monotype in side table (do not set AST::Type yet)
  annotate(E, Tv);
  return {Substitution{}, Tv};
}

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = Monotype::con("bool");
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::con("char");
  annotate(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::con("string");
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
  auto RangeT = Monotype::con("range", {TT});
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
  std::vector<std::shared_ptr<Monotype>> ArgTys;
  ArgTys.reserve(E.getArgs().size());
  for (auto &ArgUP : E.getArgs()) {
    auto [si, ti] = visit(*ArgUP);
    s.compose(si);
    tCallee = s.apply(tCallee);
    ArgTys.push_back(s.apply(ti));
  }
  auto tRes = Monotype::var(Factory.fresh());
  auto fnExpect = Monotype::fun(ArgTys, tRes);
  unifyInto(s, tCallee, fnExpect);
  recordSubst(s);
  annotate(E, s.apply(tRes));
  return {s, s.apply(tRes)};
}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [s1, tO] = visit(E.getOperand());
  Substitution s = s1;
  if (E.getOp() == TokenKind::BangKind) {
    unifyInto(s, tO, Monotype::con("bool"));
    recordSubst(s);
    annotate(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }
  // numeric unary -
  auto a = Monotype::var(Factory.fresh());
  auto opTy = Monotype::fun({a}, a);
  auto callTy = Monotype::fun({s.apply(tO)}, Monotype::var(Factory.fresh()));
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
    unifyInto(s, tL, Monotype::con("bool"));
    unifyInto(s, tR, Monotype::con("bool"));
    recordSubst(s);
    annotate(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }

  if (isComparison(K) || isEquality(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // Check if either operand is from a float literal or is a concrete float
    // type
    bool leftIsFloat =
        isFloatLiteralVar(tl) ||
        (tl->tag() == Monotype::Kind::Con &&
         (tl->getConName() == "f32" || tl->getConName() == "f64"));
    bool rightIsFloat =
        isFloatLiteralVar(tr) ||
        (tr->tag() == Monotype::Kind::Con &&
         (tr->getConName() == "f32" || tr->getConName() == "f64"));

    if (leftIsFloat || rightIsFloat) {
      // If either is a float, promote both to f64
      unifyInto(s, tl, Monotype::con("f64"));
      unifyInto(s, tr, Monotype::con("f64"));
    } else {
      // Otherwise, they must be the same type
      unifyInto(s, tl, tr);
    }

    recordSubst(s);
    annotate(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }

  if (isArithmetic(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // Check if either operand is from a float literal or is a concrete float
    // type
    bool leftIsFloat =
        isFloatLiteralVar(tl) ||
        (tl->tag() == Monotype::Kind::Con &&
         (tl->getConName() == "f32" || tl->getConName() == "f64"));
    bool rightIsFloat =
        isFloatLiteralVar(tr) ||
        (tr->tag() == Monotype::Kind::Con &&
         (tr->getConName() == "f32" || tr->getConName() == "f64"));

    if (leftIsFloat || rightIsFloat) {
      // If either is a float, promote both to f64
      unifyInto(s, tl, Monotype::con("f64"));
      unifyInto(s, tr, Monotype::con("f64"));
      recordSubst(s);
      annotate(E, Monotype::con("f64"));
      return {s, Monotype::con("f64")};
    } else {
      // Standard arithmetic unification for non-float cases
      auto a = Monotype::var(Factory.fresh());
      auto opTy = Monotype::fun({a, a}, a);
      auto callTy = Monotype::fun({tl, tr}, Monotype::var(Factory.fresh()));
      unifyInto(s, opTy, callTy);
      recordSubst(s);
      auto res = s.apply(a);
      annotate(E, res);
      return {s, res};
    }
  }

  if (K == TokenKind::EqualsKind) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // In Rust: lhs and rhs must have the same type
    unifyInto(s, tl, tr);

    recordSubst(s);

    // Assignment expressions evaluate to unit
    auto res = Monotype::con("unit");
    annotate(E, res);
    return {s, res};
  }

  throw std::runtime_error("inferBinary: unsupported operator token kind");
}
TypeInferencer::InferRes TypeInferencer::visit(StructInitExpr &E) {
  auto st = Monotype::con(E.getStructId());
  Substitution s;
  for (auto &f : E.getFields()) {
    auto [si, tv] = visit(*f->getValue());
    s.compose(si);
    if (auto *FD = f->getDecl()) {
      auto fieldTy = fromAstType(FD->getType());
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
  auto [s1, tBase] = visit(*E.getBase());
  auto out = Monotype::var(Factory.fresh());

  auto it = Structs.find(tBase->getConName());
  if (it == Structs.end()) {
    std::println("Could not find struct {} in symbol table",
                 tBase->getConName());
    return {s1, out};
  }
  StructDecl *Struct = it->second;

  FieldDecl *Field = Struct->getField(E.getMemberId());
  if (Field == nullptr) {
    std::println("Could not find field {} in struct {}", E.getMemberId(),
                 tBase->getConName());
    return {s1, out};
  }

  // Convert the field's AST type to a Monotype and unify with our output
  auto fieldType = fromAstType(Field->getType());
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
  if (tBase->tag() != Monotype::Kind::Con) {
    throw std::runtime_error("method call on non-struct type: " +
                             tBase->toString());
  }

  const std::string StructName = tBase->getConName();

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
  if (!Method) {
    throw std::runtime_error("struct '" + StructName + "' has no method '" +
                             MethodName + "'");
  }

  // 2) Build the method's Monotype from its AST param types and return type
  std::vector<std::shared_ptr<Monotype>> MethodParams;
  MethodParams.reserve(Method->getParams().size());

  for (auto &ParamUP : Method->getParams()) {
    ParamDecl *P = ParamUP.get();
    if (!P->hasType()) {
      throw std::runtime_error("method parameter '" + P->getId() +
                               "' missing type annotation");
    }
    auto PTy = fromAstType(P->getType());
    if (!PTy) {
      throw std::runtime_error(
          "internal error: fromAstType returned null for parameter of method " +
          MethodName);
    }
    MethodParams.push_back(PTy);
  }

  // Return type must be present (your FunDecl stores return type)
  auto RetTy = fromAstType(Method->getReturnTy());
  if (!RetTy) {
    throw std::runtime_error(
        "internal error: fromAstType returned null for return type of method " +
        MethodName);
  }

  // 3) Prepend the receiver type to the parameter list to form full function
  // type
  //    Here the receiver type is tBase (the concrete struct monotype).
  std::vector<std::shared_ptr<Monotype>> FullParams;
  FullParams.reserve(1 + MethodParams.size());
  FullParams.push_back(
      sBase.apply(tBase)); // apply substitution so receiver is up-to-date
  for (auto &p : MethodParams)
    FullParams.push_back(p);

  auto MethodMonotype = Monotype::fun(std::move(FullParams), RetTy);
  if (!MethodMonotype) {
    throw std::runtime_error(
        "internal error: failed to construct method monotype for " +
        StructName + "::" + MethodName);
  }

  // 4) Now infer the call: first collect argument types (receiver + explicit
  // args) Start with the base substitution sBase
  Substitution S = sBase;

  // receiver arg type (we already have it): use s-applied version
  std::vector<std::shared_ptr<Monotype>> CallArgTys;
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
  auto ResultTy = Monotype::var(Factory.fresh());

  // Expected function shape from the call site: (arg types...) -> ResultTy
  auto FnExpect = Monotype::fun(CallArgTys, ResultTy);

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
