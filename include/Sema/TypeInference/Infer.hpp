#pragma once

#include <algorithm>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"
#include "Sema/TypeInference/TypeVarFactory.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"
#include "Sema/TypeInference/Unify.hpp"

namespace phi {

class TypeInferencer {
public:
  explicit TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast);

  std::vector<std::unique_ptr<Decl>> inferProgram();

  void visit(Decl &D);
  void visit(VarDecl &D);
  void visit(FunDecl &D);
  void visit(StructDecl &D);

  using InferRes = std::pair<Substitution, Monotype>;

  InferRes visit(Stmt &S);
  InferRes visit(ReturnStmt &S);
  InferRes visit(DeferStmt &S);
  InferRes visit(ForStmt &S);
  InferRes visit(WhileStmt &S);
  InferRes visit(IfStmt &S);
  InferRes visit(DeclStmt &S);
  InferRes visit(BreakStmt &S);
  InferRes visit(ContinueStmt &S);
  InferRes visit(ExprStmt &S);

  InferRes visit(Expr &E);
  InferRes visit(IntLiteral &E);
  InferRes visit(FloatLiteral &E);
  InferRes visit(BoolLiteral &E);
  InferRes visit(CharLiteral &E);
  InferRes visit(StrLiteral &E);
  InferRes visit(RangeLiteral &E);
  InferRes visit(DeclRefExpr &E);
  InferRes visit(FunCallExpr &E);
  InferRes visit(BinaryOp &E);
  InferRes visit(UnaryOp &E);
  InferRes visit(StructInitExpr &E);
  InferRes visit(FieldInitExpr &E);
  InferRes visit(FieldAccessExpr &E);
  InferRes visit(MethodCallExpr &E);

private:
  std::vector<std::unique_ptr<Decl>> Ast;
  TypeEnv Env;
  TypeVarFactory Factory;

  std::unordered_map<std::string, StructDecl *> Structs;

  // Accumulate all substitutions produced during inference so we can finalize.
  Substitution GlobalSubst;

  // Side tables: store the HM monotypes for nodes until finalization.
  std::unordered_map<Expr *, Monotype> ExprMonos;
  std::unordered_map<ValueDecl *, Monotype> ValDeclMonos;
  std::unordered_map<FunDecl *, Monotype> FunDeclMonos;

  // track integer-literal-origin type variables (so we can default them later)
  std::vector<TypeVar> IntTypeVars;
  std::vector<TypeVar> FloatTypeVars;

  // expected return type stack
  std::vector<Monotype> CurFunRetType;

  // main passes
  void predeclare();

  // statements / blocks
  InferRes inferBlock(Block &B);

  // helpers
  static void unifyInto(Substitution &S, const Monotype &A, const Monotype &B) {
    Substitution U = unify(S.apply(A), S.apply(B));
    S.compose(U);
  }

  // *** annotation helpers (now side-table based) ***
  void annotate(ValueDecl &D, const Monotype &T);
  void annotate(Expr &E, const Monotype &T);

  // record and propagate substitutions into global state + env
  void recordSubst(const Substitution &S);

  void defaultNums();

  // After inference & defaulting, finalize by applying GlobalSubst
  // to stored Monotypes and writing concrete phi::Type back into AST.
  void finalizeAnnotations();

  // Helper function to check if a type variable comes from a float literal
  [[nodiscard]] bool isFloatLiteralVar(const Monotype &T) const {
    if (!T.isVar())
      return false;
    return std::ranges::contains(FloatTypeVars, T.asVar());
  }

  // Helper function to check if a type variable comes from an int literal
  [[nodiscard]] bool isIntLiteralVar(const Monotype &T) const {
    if (!T.isVar())
      return false;
    return std::ranges::contains(IntTypeVars, T.asVar());
  }

  // token-kind helpers (same as before)
  [[nodiscard]] static bool isArithmetic(TokenKind K) noexcept;
  [[nodiscard]] static bool isLogical(TokenKind K) noexcept;
  [[nodiscard]] static bool isComparison(TokenKind K) noexcept;
  [[nodiscard]] static bool isEquality(TokenKind K) noexcept;
};

} // namespace phi
