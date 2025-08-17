#pragma once

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include "Sema/HMTI/HMType.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <memory>
#include <vector>

namespace phi {

class Infer {
public:
  explicit Infer(std::vector<std::unique_ptr<Decl>> Ast)
      : Ast_(std::move(Ast)) {}

  void inferProgram();

private:
  std::vector<std::unique_ptr<Decl>> Ast_;
  TypeEnv Env_;
  TypeVarFactory Factory_;

  // tracks integer literal type variables (for defaulting to i32)
  std::vector<TypeVar> IntLiteralVars_;

  // expected return type stack
  std::vector<std::shared_ptr<Monotype>> CurrentFnReturnTy_;

  using InferRes = std::pair<Substitution, std::shared_ptr<Monotype>>;

  // main passes
  void predeclare();
  void inferDecl(Decl &D);
  void inferVarDecl(VarDecl &D);
  void inferFunDecl(FunDecl &D);

  // expr inference
  InferRes inferExpr(Expr &E);
  InferRes inferIntLiteral(IntLiteral &E);
  InferRes inferFloatLiteral(FloatLiteral &E);
  InferRes inferBoolLiteral(BoolLiteral &E);
  InferRes inferCharLiteral(CharLiteral &E);
  InferRes inferStrLiteral(StrLiteral &E);
  InferRes inferRangeLiteral(RangeLiteral &E);
  InferRes inferDeclRef(DeclRefExpr &E);
  InferRes inferCall(FunCallExpr &E);
  InferRes inferBinary(BinaryOp &E);
  InferRes inferUnary(UnaryOp &E);
  InferRes inferStructInit(StructInitExpr &E);
  InferRes inferFieldInit(FieldInitExpr &E);
  InferRes inferMemberAccess(MemberAccessExpr &E);
  InferRes inferMemberFunCall(MemberFunCallExpr &E);

  // statements / blocks
  InferRes inferStmt(Stmt &S);
  InferRes inferBlock(Block &B);

  // helpers
  static void unifyInto(Substitution &S, const std::shared_ptr<Monotype> &A,
                        const std::shared_ptr<Monotype> &B) {
    Substitution U = unify(S.apply(A), S.apply(B));
    S.compose(U);
  }

  std::shared_ptr<Monotype> typeFromAstOrFresh(std::optional<Type> AstTyOpt);

  // adapters
  std::shared_ptr<Monotype> fromAstType(const Type &T);
  Type toAstType(const std::shared_ptr<Monotype> &T);

  // annotate AST nodes
  void annotate(ValueDecl &D, const std::shared_ptr<Monotype> &T);
  void annotate(FunDecl &D, const std::shared_ptr<Monotype> &FnT);
  void annotateExpr(Expr &E, const std::shared_ptr<Monotype> &T);

  // integer defaulting
  void defaultIntegerLiterals();

  // token-kind helpers (adapt to your TokenKind names if different)
  bool isArithmetic(TokenKind K) const noexcept;
  bool isLogical(TokenKind K) const noexcept;
  bool isComparison(TokenKind K) const noexcept;
  bool isEquality(TokenKind K) const noexcept;
};

} // namespace phi
