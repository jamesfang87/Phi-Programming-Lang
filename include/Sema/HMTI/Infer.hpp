#pragma once

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Sema/HMTI/HMType.hpp"
#include "Sema/HMTI/TypeEnv.hpp"
#include "SrcManager/SrcLocation.hpp"

#include <format>
#include <memory>
#include <string>
#include <unordered_map>
#include <vector>

namespace phi {

class TypeInferencer {
public:
  explicit TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast);

  std::vector<std::unique_ptr<Decl>> inferProgram();

  InferRes visit(Stmt &S);
  InferRes visit(ReturnStmt &S);
  InferRes visit(ForStmt &S);
  InferRes visit(WhileStmt &S);
  InferRes visit(IfStmt &S);
  InferRes visit(DeclStmt &S);
  InferRes visit(BreakStmt &S);
  InferRes visit(ContinueStmt &S);

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
  InferRes visit(MemberAccessExpr &E);
  InferRes visit(MemberFunCallExpr &E);

private:
  std::vector<std::unique_ptr<Decl>> Ast;
  TypeEnv Env;
  TypeVarFactory Factory;

  std::unordered_map<std::string, StructDecl *> Structs;

  // Accumulate all substitutions produced during inference so we can finalize.
  Substitution GlobalSubst;

  // Side tables: store the HM monotypes for nodes until finalization.
  std::unordered_map<Expr *, std::shared_ptr<Monotype>> ExprMonos;
  std::unordered_map<ValueDecl *, std::shared_ptr<Monotype>> ValDeclMonos;
  std::unordered_map<FunDecl *, std::shared_ptr<Monotype>> FunDeclMonos;

  // track integer-literal-origin type variables (so we can default them later)
  std::vector<TypeVar> IntLiteralVars;
  std::vector<TypeVar> FloatLiteralVars;

  // expected return type stack
  std::vector<std::shared_ptr<Monotype>> CurrentFnReturnTy;

  using InferRes = std::pair<Substitution, std::shared_ptr<Monotype>>;

  // main passes
  void predeclare();
  void inferDecl(Decl &D);
  void inferVarDecl(VarDecl &D);
  void inferFunDecl(FunDecl &D);

  // statements / blocks

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

  // *** annotation helpers (now side-table based) ***
  void annotate(ValueDecl &D, const std::shared_ptr<Monotype> &T);
  void annotate(Expr &E, const std::shared_ptr<Monotype> &T);

  // record and propagate substitutions into global state + env
  void recordSubst(const Substitution &S);

  void defaultNums();

  // After inference & defaulting, finalize by applying GlobalSubst_
  // to stored Monotypes and writing concrete phi::Type back into AST.
  void finalizeAnnotations();
  struct IntConstraint {
    TypeVar Var;
    SrcLocation Loc;
  };
  std::vector<IntConstraint> IntRangeVars;

  // Helper function to check if a type variable comes from a float literal
  bool isFloatLiteralVar(const std::shared_ptr<Monotype> &t) const {
    if (t->tag() != Monotype::Kind::Var)
      return false;
    auto var = t->asVar();
    return std::find(FloatLiteralVars.begin(), FloatLiteralVars.end(), var) !=
           FloatLiteralVars.end();
  }

  // Helper function to check if a type variable comes from an int literal
  bool isIntLiteralVar(const std::shared_ptr<Monotype> &t) const {
    if (t->tag() != Monotype::Kind::Var)
      return false;
    auto var = t->asVar();
    return std::find(IntLiteralVars.begin(), IntLiteralVars.end(), var) !=
           IntLiteralVars.end();
  }

  // Add new method:
  void checkIntegerConstraints() {
    for (const auto &Constraint : IntRangeVars) {
      auto T = GlobalSubst.apply(Monotype::var(Constraint.Var));
      if (!T->isIntType()) {
        auto [ignore, Line, Col] = Constraint.Loc;
        std::string Msg = std::format(
            "Loop variable must be integer type, got: {} at location {}:{}",
            T->toString(), Line, Col);

        throw std::runtime_error(Msg);
      }
    }
  }

  // token-kind helpers (same as before)
  [[nodiscard]] bool isArithmetic(TokenKind K) const noexcept;
  [[nodiscard]] bool isLogical(TokenKind K) const noexcept;
  [[nodiscard]] bool isComparison(TokenKind K) const noexcept;
  [[nodiscard]] bool isEquality(TokenKind K) const noexcept;
};

} // namespace phi
