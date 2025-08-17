#include "Sema/HMTI/Infer.hpp"

namespace phi {

// ---------------- Expr inference ----------------
// Each inferX returns (Substitution, Monotype). Caller must record/apply
// Substitution.

TypeInferencer::InferRes TypeInferencer::visit(IntLiteral &E) {
  // create fresh type variable for integer literal (polymorphic int)
  auto Tv = Monotype::var(Factory_.fresh());
  IntLiteralVars_.push_back(Tv->asVar());
  // store monotype in side table (do not set AST::Type yet)
  annotateExpr(E, Tv);
  return {Substitution{}, Tv};
}

TypeInferencer::InferRes TypeInferencer::visit(FloatLiteral &E) {
  // create fresh type variable for integer literal (polymorphic int)
  auto Tv = Monotype::var(Factory_.fresh());
  FloatLiteralVars_.push_back(Tv->asVar());
  // store monotype in side table (do not set AST::Type yet)
  annotateExpr(E, Tv);
  return {Substitution{}, Tv};
}

TypeInferencer::InferRes TypeInferencer::visit(BoolLiteral &E) {
  auto T = Monotype::con("bool");
  annotateExpr(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(CharLiteral &E) {
  auto T = Monotype::con("char");
  annotateExpr(E, T);
  return {Substitution{}, T};
}

TypeInferencer::InferRes TypeInferencer::visit(StrLiteral &E) {
  auto T = Monotype::con("string");
  annotateExpr(E, T);
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
  annotateExpr(E, RangeT);
  return {S, RangeT};
}

TypeInferencer::InferRes TypeInferencer::visit(DeclRefExpr &E) {
  if (auto *VD = E.getDecl()) {
    auto Sc = Env_.lookup(VD);
    if (!Sc)
      throw std::runtime_error("unbound declaration: " + E.getId());
    auto T = instantiate(*Sc, Factory_);
    annotateExpr(E, T);
    return {Substitution{}, T};
  }
  // fallback by name (could be a function or var)
  auto Scn = Env_.lookup(E.getId());
  if (!Scn)
    throw std::runtime_error("unbound identifier: " + E.getId());
  auto T = instantiate(*Scn, Factory_);
  annotateExpr(E, T);
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
  auto tRes = Monotype::var(Factory_.fresh());
  auto fnExpect = Monotype::fun(ArgTys, tRes);
  unifyInto(s, tCallee, fnExpect);
  recordSubst(s);
  annotateExpr(E, s.apply(tRes));
  return {s, s.apply(tRes)};
}

TypeInferencer::InferRes TypeInferencer::visit(UnaryOp &E) {
  auto [s1, tO] = visit(E.getOperand());
  Substitution s = s1;
  if (E.getOp() == TokenKind::BangKind) {
    unifyInto(s, tO, Monotype::con("bool"));
    recordSubst(s);
    annotateExpr(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }
  // numeric unary -
  auto a = Monotype::var(Factory_.fresh());
  auto opTy = Monotype::fun({a}, a);
  auto callTy = Monotype::fun({s.apply(tO)}, Monotype::var(Factory_.fresh()));
  unifyInto(s, opTy, callTy);
  recordSubst(s);
  annotateExpr(E, s.apply(a));
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
    annotateExpr(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }

  if (isComparison(K) || isEquality(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // Check if either operand is from a float literal or is a concrete float
    // type
    bool leftIsFloat = isFloatLiteralVar(tl) ||
                       (tl->tag() == Monotype::Tag::Con &&
                        (tl->conName() == "f32" || tl->conName() == "f64"));
    bool rightIsFloat = isFloatLiteralVar(tr) ||
                        (tr->tag() == Monotype::Tag::Con &&
                         (tr->conName() == "f32" || tr->conName() == "f64"));

    if (leftIsFloat || rightIsFloat) {
      // If either is a float, promote both to f64
      unifyInto(s, tl, Monotype::con("f64"));
      unifyInto(s, tr, Monotype::con("f64"));
    } else {
      // Otherwise, they must be the same type
      unifyInto(s, tl, tr);
    }

    recordSubst(s);
    annotateExpr(E, Monotype::con("bool"));
    return {s, Monotype::con("bool")};
  }

  if (isArithmetic(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);

    // Check if either operand is from a float literal or is a concrete float
    // type
    bool leftIsFloat = isFloatLiteralVar(tl) ||
                       (tl->tag() == Monotype::Tag::Con &&
                        (tl->conName() == "f32" || tl->conName() == "f64"));
    bool rightIsFloat = isFloatLiteralVar(tr) ||
                        (tr->tag() == Monotype::Tag::Con &&
                         (tr->conName() == "f32" || tr->conName() == "f64"));

    if (leftIsFloat || rightIsFloat) {
      // If either is a float, promote both to f64
      unifyInto(s, tl, Monotype::con("f64"));
      unifyInto(s, tr, Monotype::con("f64"));
      recordSubst(s);
      annotateExpr(E, Monotype::con("f64"));
      return {s, Monotype::con("f64")};
    } else {
      // Standard arithmetic unification for non-float cases
      auto a = Monotype::var(Factory_.fresh());
      auto opTy = Monotype::fun({a, a}, a);
      auto callTy = Monotype::fun({tl, tr}, Monotype::var(Factory_.fresh()));
      unifyInto(s, opTy, callTy);
      recordSubst(s);
      auto res = s.apply(a);
      annotateExpr(E, res);
      return {s, res};
    }
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
  annotateExpr(E, st);
  return {s, st};
}

TypeInferencer::InferRes TypeInferencer::visit(FieldInitExpr &E) {
  auto [s, tv] = visit(*E.getValue());
  recordSubst(s);
  annotateExpr(E, tv);
  return {s, tv};
}

TypeInferencer::InferRes TypeInferencer::visit(MemberAccessExpr &E) {
  auto [s1, tBase] = visit(*E.getBase());
  auto out = Monotype::var(Factory_.fresh());
  // If you have resolved FieldDecl on this node, unify here:
  // if (auto *FD = E.getResolvedFieldDecl()) unifyInto(s1, out,
  // fromAstType(FD->getType()));
  recordSubst(s1);
  annotateExpr(E, out);
  return {s1, out};
}

TypeInferencer::InferRes TypeInferencer::visit(MemberFunCallExpr &E) {
  auto [sB, tB] = visit(*E.getBase());
  auto [sC, tC] = visit(E.getCall());
  Substitution s = sC;
  s.compose(sB);
  recordSubst(s);
  annotateExpr(E, tC);
  return {s, tC};
}

TypeInferencer::InferRes TypeInferencer::visit(Expr &E) {
  return E.accept(*this);
}

} // namespace phi
