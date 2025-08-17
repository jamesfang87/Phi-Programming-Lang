// src/Sema/HMTI/Infer.cpp
#include "Sema/HMTI/Infer.hpp"
#include "Sema/HMTI/Adapters/TypeAdapters.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <stdexcept>

namespace phi {

// ----- token-kind helpers (adapt to actual TokenKind enum) -----
bool Infer::isArithmetic(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::PlusKind:
  case TokenKind::MinusKind:
  case TokenKind::StarKind:
  case TokenKind::SlashKind:
    return true;
  default:
    return false;
  }
}
bool Infer::isLogical(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::DoubleAmpKind:
  case TokenKind::DoublePipeKind:
    return true;
  default:
    return false;
  }
}
bool Infer::isComparison(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::OpenCaretKind:
  case TokenKind::LessEqualKind:
  case TokenKind::CloseCaretKind:
  case TokenKind::GreaterEqualKind:
    return true;
  default:
    return false;
  }
}
bool Infer::isEquality(TokenKind K) const noexcept {
  switch (K) {
  case TokenKind::DoubleEqualsKind:
  case TokenKind::BangEqualsKind:
    return true;
  default:
    return false;
  }
}

// ---------------- top-level program ----------------
void Infer::inferProgram() {
  predeclare();
  for (auto &Up : Ast_)
    inferDecl(*Up);
  defaultIntegerLiterals();
}

// Predeclare binds variables by Decl pointer and functions by name
void Infer::predeclare() {
  for (auto &Up : Ast_) {
    if (auto *V = dynamic_cast<VarDecl *>(Up.get())) {
      // top-level var: give fresh var for letrec
      auto Tv = Monotype::var(Factory_.fresh());
      Env_.bind(V, Polytype{{}, Tv});
    } else if (auto *F = dynamic_cast<FunDecl *>(Up.get())) {
      // functions must be annotated by the user (params & return)
      // build function monotype from user annotations (no fresh vars)
      std::vector<std::shared_ptr<Monotype>> ArgTys;
      ArgTys.reserve(F->getParams().size());
      for (auto &Pup : F->getParams()) {
        ParamDecl *P = Pup.get();
        if (!P->hasType()) {
          throw std::runtime_error("Function parameter '" + P->getId() +
                                   "' must be annotated with a type.");
        }
        ArgTys.push_back(fromAstType(P->getType()));
      }
      // FunDecl::getReturnTy() in your AST is a concrete Type (user must
      // annotate)
      auto Ret = fromAstType(F->getReturnTy());
      auto FnT = Monotype::fun(std::move(ArgTys), Ret);

      // Bind function by name in the environment (top-level generalization)
      Env_.bind(F->getId(), generalize(Env_, FnT));
    }
  }
}

// ---------------- declarations ----------------
void Infer::inferDecl(Decl &D) {
  if (auto *V = dynamic_cast<VarDecl *>(&D))
    inferVarDecl(*V);
  else if (auto *F = dynamic_cast<FunDecl *>(&D))
    inferFunDecl(*F);
  // struct/field/method handled elsewhere
}

// VarDecl: infer initializer, unify with annotation (if present), generalize
void Infer::inferVarDecl(VarDecl &D) {
  auto ScOpt = Env_.lookup(&D);
  auto VarTy =
      ScOpt ? instantiate(*ScOpt, Factory_) : Monotype::var(Factory_.fresh());

  Substitution S;
  if (D.hasInit()) {
    auto [Si, Ti] = inferExpr(D.getInit());
    S = std::move(Si);
    VarTy = S.apply(VarTy);
    unifyInto(S, VarTy, Ti);
  }

  if (D.hasType()) {
    auto Ann = fromAstType(D.getType());
    unifyInto(S, VarTy, Ann);
    VarTy = S.apply(Ann);
  } else {
    VarTy = S.apply(VarTy);
  }

  Env_.applySubstitution(S);

  auto Sc = generalize(Env_, VarTy);
  Env_.bind(&D, Sc);

  annotate(D, VarTy);
}

// FunDecl: instantiate predeclared scheme (by name), bind params, infer body,
// check that returns conform to declared return type (user-provided), and
// re-generalize.
void Infer::inferFunDecl(FunDecl &D) {
  std::shared_ptr<Monotype> FnT;

  // Lookup by name (FunDecl is not a ValueDecl)
  if (auto Sc = Env_.lookup(D.getId())) {
    FnT = instantiate(*Sc, Factory_);
  } else {
    // As a fallback (shouldn't happen if predeclare ran), build from AST
    // annotations. But since functions must be annotated by the user, require
    // param & return annotations.
    std::vector<std::shared_ptr<Monotype>> Args;
    Args.reserve(D.getParams().size());
    for (auto &Pup : D.getParams()) {
      ParamDecl *P = Pup.get();
      if (!P->hasType()) {
        throw std::runtime_error("Missing parameter type annotation for '" +
                                 P->getId() + "'");
      }
      Args.push_back(fromAstType(P->getType()));
    }
    if (!D.getReturnTy().isPrimitive() &&
        D.getReturnTy().getPrimitiveType() == Type::PrimitiveKind::CustomKind) {
      // Normal flow: user provided return type through constructor
    }
    auto Ret = fromAstType(D.getReturnTy());
    FnT = Monotype::fun(std::move(Args), Ret);
  }

  if (FnT->tag() != Monotype::Tag::Fun)
    throw std::runtime_error("internal: function expected a function monotype");

  // Save outer env, then extend with params for body inference
  auto SavedEnv = Env_;

  // Bind parameters to their declared types (user must annotate params)
  for (size_t i = 0; i < D.getParams().size(); ++i) {
    ParamDecl *P = D.getParams()[i].get();
    if (!P->hasType()) {
      throw std::runtime_error("Parameter '" + P->getId() + "' in function '" +
                               D.getId() + "' must have a type annotation");
    }
    auto Pt = fromAstType(P->getType());
    Env_.bind(P, Polytype{{}, Pt});
    // Do not overwrite user's annotation on the param; we only bind it for
    // lookup in the body.
  }

  // Use the declared return type as the expected return for the body
  // (user-provided)
  auto DeclaredRet = fromAstType(D.getReturnTy());
  CurrentFnReturnTy_.push_back(DeclaredRet);

  // Infer body
  auto [SBody, _] = inferBlock(D.getBody());
  // Apply substitution to current env (which contains param bindings)
  Env_.applySubstitution(SBody);

  // Apply body substitution to function type
  FnT = SBody.apply(FnT);

  CurrentFnReturnTy_.pop_back();

  // Verify that final function type matches declarations:
  // params and return must match user annotations (unify to detect mismatch)
  try {
    // unify parameter types
    for (size_t i = 0; i < D.getParams().size(); ++i) {
      ParamDecl *P = D.getParams()[i].get();
      auto DeclParamTy = fromAstType(P->getType());
      unifyInto(SBody, DeclParamTy, FnT->funArgs()[i]);
    }
    // unify return type (defensive; returns already checked during traversal)
    auto DeclRetTy = fromAstType(D.getReturnTy());
    unifyInto(SBody, DeclRetTy, FnT->funRet());
  } catch (const UnifyError &E) {
    // Attach source location if you want; here we rethrow as runtime_error
    throw std::runtime_error(std::string("Type error in function '") +
                             D.getId() + "': " + E.what());
  }

  // Re-generalize in outer (saved) environment and re-bind the function name
  auto Sc = generalize(SavedEnv, FnT);
  SavedEnv.bind(D.getId(), Sc);

  // Restore env to saved (we don't keep param bindings in the outer env)
  Env_ = std::move(SavedEnv);

  // Do NOT modify the AST's declared return/param types — user-provided
  // annotations are authoritative. We don't call D.setReturnTy(...) because
  // your FunDecl does not expose that setter and per design the user must
  // supply those annotations.
}

// ---------------- Expr inference (literal leaves) ----------------
Infer::InferRes Infer::inferIntLiteral(IntLiteral &E) {
  auto Tv = Monotype::var(Factory_.fresh());
  IntLiteralVars_.push_back(Tv->asVar());
  annotateExpr(E, Tv);
  return {Substitution{}, Tv};
}
Infer::InferRes Infer::inferFloatLiteral(FloatLiteral &E) {
  auto T = Monotype::con("f64");
  annotateExpr(E, T);
  return {Substitution{}, T};
}
Infer::InferRes Infer::inferBoolLiteral(BoolLiteral &E) {
  auto T = makeBool();
  annotateExpr(E, T);
  return {Substitution{}, T};
}
Infer::InferRes Infer::inferCharLiteral(CharLiteral &E) {
  auto T = makeChar();
  annotateExpr(E, T);
  return {Substitution{}, T};
}
Infer::InferRes Infer::inferStrLiteral(StrLiteral &E) {
  auto T = makeString();
  annotateExpr(E, T);
  return {Substitution{}, T};
}

// Range literal: unify start and end; produce range<T>
Infer::InferRes Infer::inferRangeLiteral(RangeLiteral &E) {
  auto [s1, tStart] = inferExpr(E.getStart());
  auto [s2, tEnd] = inferExpr(E.getEnd());
  Substitution s = s2;
  s.compose(s1);
  unifyInto(s, tStart, tEnd);
  auto tt = s.apply(tStart);
  auto rangeTy = Monotype::con("range", {tt});
  annotateExpr(E, rangeTy);
  return {s, rangeTy};
}

// DeclRefExpr: prefer resolved ValueDecl, else lookup by name (may find vars or
// functions)
Infer::InferRes Infer::inferDeclRef(DeclRefExpr &E) {
  if (auto *VD = E.getDecl()) {
    auto Sc = Env_.lookup(VD);
    if (!Sc)
      throw std::runtime_error("unbound declaration: " + E.getId());
    auto T = instantiate(*Sc, Factory_);
    annotateExpr(E, T);
    return {Substitution{}, T};
  }
  // fallback by name: value or function
  auto Scn = Env_.lookup(E.getId());
  if (!Scn)
    throw std::runtime_error("unbound identifier: " + E.getId());
  auto T = instantiate(*Scn, Factory_);
  annotateExpr(E, T);
  return {Substitution{}, T};
}

// Fun call: infer callee & args; unify
Infer::InferRes Infer::inferCall(FunCallExpr &E) {
  auto [sC, tCallee] = inferExpr(E.getCallee());
  Substitution s = sC;
  std::vector<std::shared_ptr<Monotype>> argTys;
  argTys.reserve(E.getArgs().size());
  for (auto &ArgUP : E.getArgs()) {
    auto [si, ti] = inferExpr(*ArgUP);
    s.compose(si);
    tCallee = s.apply(tCallee);
    argTys.push_back(s.apply(ti));
  }
  auto tRes = Monotype::var(Factory_.fresh());
  auto fnExpect = Monotype::fun(argTys, tRes);
  unifyInto(s, tCallee, fnExpect);
  annotateExpr(E, s.apply(tRes));
  return {s, s.apply(tRes)};
}

// Unary: logical not => bool->bool, numeric unary - => (α)->α
Infer::InferRes Infer::inferUnary(UnaryOp &E) {
  auto [s1, tO] = inferExpr(E.getOperand());
  Substitution s = s1;
  // assume TokenKind::Bang is logical not
  if (E.getOp() == TokenKind::BangKind) {
    unifyInto(s, tO, makeBool());
    annotateExpr(E, makeBool());
    return {s, makeBool()};
  }
  auto a = Monotype::var(Factory_.fresh());
  auto opTy = Monotype::fun({a}, a);
  auto callTy = Monotype::fun({s.apply(tO)}, Monotype::var(Factory_.fresh()));
  unifyInto(s, opTy, callTy);
  annotateExpr(E, s.apply(a));
  return {s, s.apply(a)};
}

// Binary operations: logical, comparison (-> bool), arithmetic (float
// promotion)
Infer::InferRes Infer::inferBinary(BinaryOp &E) {
  auto [sL, tL] = inferExpr(E.getLhs());
  auto [sR, tR] = inferExpr(E.getRhs());
  Substitution s = sR;
  s.compose(sL);

  TokenKind K = E.getOp();

  if (isLogical(K)) {
    unifyInto(s, tL, makeBool());
    unifyInto(s, tR, makeBool());
    annotateExpr(E, makeBool());
    return {s, makeBool()};
  }

  if (isComparison(K) || isEquality(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);
    bool eitherFloat = (tl->tag() == Monotype::Tag::Con &&
                        (tl->conName() == "f32" || tl->conName() == "f64")) ||
                       (tr->tag() == Monotype::Tag::Con &&
                        (tr->conName() == "f32" || tr->conName() == "f64"));
    if (eitherFloat) {
      unifyInto(s, tl, makeF64());
      unifyInto(s, tr, makeF64());
    } else {
      unifyInto(s, tl, tr);
    }
    annotateExpr(E, makeBool());
    return {s, makeBool()};
  }

  if (isArithmetic(K)) {
    auto tl = s.apply(tL);
    auto tr = s.apply(tR);
    auto a = Monotype::var(Factory_.fresh());
    auto opTy = Monotype::fun({a, a}, a);
    auto callTy = Monotype::fun({tl, tr}, Monotype::var(Factory_.fresh()));
    unifyInto(s, opTy, callTy);
    tl = s.apply(tl);
    tr = s.apply(tr);
    // float promotion
    bool eitherFloat = (tl->tag() == Monotype::Tag::Con &&
                        (tl->conName() == "f32" || tl->conName() == "f64")) ||
                       (tr->tag() == Monotype::Tag::Con &&
                        (tr->conName() == "f32" || tr->conName() == "f64"));
    if (eitherFloat) {
      unifyInto(s, tl, makeF64());
      unifyInto(s, tr, makeF64());
      annotateExpr(E, makeF64());
      return {s, makeF64()};
    }
    auto res = s.apply(a);
    annotateExpr(E, res);
    return {s, res};
  }

  throw std::runtime_error("inferBinary: unsupported operator");
}

// Struct init: unify each field init with declared field type if FieldDecl is
// present
Infer::InferRes Infer::inferStructInit(StructInitExpr &E) {
  auto st = Monotype::con(E.getStructId());
  Substitution s;
  for (auto &f : E.getFields()) {
    auto [si, tv] = inferExpr(*f->getValue());
    s.compose(si);
    if (auto *FD = f->getDecl()) {
      auto fieldTy = fromAstType(FD->getType());
      unifyInto(s, fieldTy, tv);
    }
  }
  annotateExpr(E, st);
  return {s, st};
}

Infer::InferRes Infer::inferFieldInit(FieldInitExpr &E) {
  auto [s, tv] = inferExpr(*E.getValue());
  annotateExpr(E, tv);
  return {s, tv};
}

Infer::InferRes Infer::inferMemberAccess(MemberAccessExpr &E) {
  auto [s1, tBase] = inferExpr(*E.getBase());
  // conservative approach: produce a fresh var and unify with resolved field
  // decl if present
  auto out = Monotype::var(Factory_.fresh());
  // if you resolve FieldDecl on the MemberAccess node, unify here:
  // if (auto *FD = E.getResolvedFieldDecl()) unifyInto(s1, out,
  // fromAstType(FD->getType()));
  annotateExpr(E, out);
  return {s1, out};
}

Infer::InferRes Infer::inferMemberFunCall(MemberFunCallExpr &E) {
  auto [sB, tB] = inferExpr(*E.getBase());
  auto [sC, tC] = inferExpr(E.getCall());
  Substitution s = sC;
  s.compose(sB);
  annotateExpr(E, tC);
  return {s, tC};
}

// Dispatcher
Infer::InferRes Infer::inferExpr(Expr &E) {
  if (auto *I = dynamic_cast<IntLiteral *>(&E))
    return inferIntLiteral(*I);
  if (auto *F = dynamic_cast<FloatLiteral *>(&E))
    return inferFloatLiteral(*F);
  if (auto *B = dynamic_cast<BoolLiteral *>(&E))
    return inferBoolLiteral(*B);
  if (auto *C = dynamic_cast<CharLiteral *>(&E))
    return inferCharLiteral(*C);
  if (auto *S = dynamic_cast<StrLiteral *>(&E))
    return inferStrLiteral(*S);
  if (auto *R = dynamic_cast<RangeLiteral *>(&E))
    return inferRangeLiteral(*R);
  if (auto *DR = dynamic_cast<DeclRefExpr *>(&E))
    return inferDeclRef(*DR);
  if (auto *FC = dynamic_cast<FunCallExpr *>(&E))
    return inferCall(*FC);
  if (auto *BO = dynamic_cast<BinaryOp *>(&E))
    return inferBinary(*BO);
  if (auto *UO = dynamic_cast<UnaryOp *>(&E))
    return inferUnary(*UO);
  if (auto *SI = dynamic_cast<StructInitExpr *>(&E))
    return inferStructInit(*SI);
  if (auto *MA = dynamic_cast<MemberAccessExpr *>(&E))
    return inferMemberAccess(*MA);
  if (auto *MFC = dynamic_cast<MemberFunCallExpr *>(&E))
    return inferMemberFunCall(*MFC);

  throw std::runtime_error("inferExpr: unhandled Expr kind");
}

// ---------------- Block / Stmt ----------------
Infer::InferRes Infer::inferBlock(Block &B) {
  Substitution s;
  for (auto &Up : B.getStmts()) {
    auto [si, _] = inferStmt(*Up);
    s.compose(si);
  }
  return {s, makeUnit()};
}

Infer::InferRes Infer::inferStmt(Stmt &S) {
  if (auto *RS = dynamic_cast<ReturnStmt *>(&S)) {
    auto Expected =
        CurrentFnReturnTy_.empty() ? makeUnit() : CurrentFnReturnTy_.back();
    if (RS->hasExpr()) {
      auto [si, t] = inferExpr(RS->getExpr());
      unifyInto(si, t, Expected);
      return {si, makeUnit()};
    } else {
      Substitution s0;
      unifyInto(s0, makeUnit(), Expected);
      return {s0, makeUnit()};
    }
  }

  if (auto *DS = dynamic_cast<DeclStmt *>(&S)) {
    inferDecl(DS->getDecl());
    return {Substitution{}, makeUnit()};
  }

  if (auto *ES = dynamic_cast<Expr *>(&S)) {
    auto [si, _] = inferExpr(*ES);
    return {si, makeUnit()};
  }

  if (auto *IS = dynamic_cast<IfStmt *>(&S)) {
    auto [sc, tc] = inferExpr(IS->getCond());
    Substitution sacc = sc;
    unifyInto(sacc, tc, makeBool());
    auto [st, _] = inferBlock(IS->getThen());
    sacc.compose(st);
    if (IS->hasElse()) {
      auto [se, __] = inferBlock(IS->getElse());
      sacc.compose(se);
    }
    return {sacc, makeUnit()};
  }

  if (auto *WS = dynamic_cast<WhileStmt *>(&S)) {
    auto [sc, tc] = inferExpr(WS->getCond());
    Substitution sacc = sc;
    unifyInto(sacc, tc, makeBool());
    auto [sb, _] = inferBlock(WS->getBody());
    sacc.compose(sb);
    return {sacc, makeUnit()};
  }

  throw std::runtime_error("inferStmt: unhandled Stmt kind");
}

// ---------------- Adapters / Annotations ----------------
std::shared_ptr<Monotype>
Infer::typeFromAstOrFresh(std::optional<Type> AstTyOpt) {
  if (!AstTyOpt.has_value())
    return Monotype::var(Factory_.fresh());
  return fromAstType(*AstTyOpt);
}

std::shared_ptr<Monotype> Infer::fromAstType(const Type &T) {
  return ::phi::fromAstType(T);
}
Type Infer::toAstType(const std::shared_ptr<Monotype> &T) {
  return ::phi::toAstType(T);
}

void Infer::annotate(ValueDecl &D, const std::shared_ptr<Monotype> &T) {
  D.setType(toAstType(T));
}

// For functions we *don't* mutate the user's annotations. Provide a verifier
// only.
void Infer::annotate(FunDecl &D, const std::shared_ptr<Monotype> &FnT) {
  // Do not modify D's return or param types. We only verify consistency.
  if (FnT->tag() != Monotype::Tag::Fun)
    return;

  // Verify param counts match
  if (FnT->funArgs().size() != D.getParams().size()) {
    throw std::runtime_error(
        "internal: parameter arity mismatch when verifying function '" +
        D.getId() + "'");
  }

  // Verify each declared param type matches the inferred param monotype
  for (size_t i = 0; i < D.getParams().size(); ++i) {
    ParamDecl *P = D.getParams()[i].get();
    if (!P->hasType()) {
      throw std::runtime_error("Parameter '" + P->getId() +
                               "' must be annotated");
    }
    auto DeclParamTy = fromAstType(P->getType());
    // if unify fails, it'll throw UnifyError
    Substitution S = unify(DeclParamTy, FnT->funArgs()[i]);
    (void)S;
  }

  // Verify return type
  auto DeclRet = fromAstType(D.getReturnTy());
  Substitution Sret = unify(DeclRet, FnT->funRet());
  (void)Sret;
}

void Infer::annotateExpr(Expr &E, const std::shared_ptr<Monotype> &T) {
  E.setType(toAstType(T));
}

// ---------------- integer defaulting ----------------
void Infer::defaultIntegerLiterals() {
  for (const auto &V : IntLiteralVars_) {
    Substitution S;
    S.Map.emplace(V, Monotype::con("i32"));
    Env_.applySubstitution(S);
    // If you want expression nodes updated after this, run a re-annotation pass
    // that maps side-table Monotype -> AST Type and writes Expr::setType again.
  }
}

} // namespace phi
