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
#include <unordered_map>
#include <vector>

namespace phi {

class TypeInferencer {
public:
  explicit TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast)
      : Ast_(std::move(Ast)) {}

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
  std::vector<std::unique_ptr<Decl>> Ast_;
  TypeEnv Env_;
  TypeVarFactory Factory_;

  // Accumulate all substitutions produced during inference so we can finalize.
  Substitution GlobalSubst_;

  // Side tables: store the HM monotypes for nodes until finalization.
  std::unordered_map<const Expr *, std::shared_ptr<Monotype>> ExprMonos_;
  std::unordered_map<const ValueDecl *, std::shared_ptr<Monotype>> DeclMonos_;
  std::unordered_map<const FunDecl *, std::shared_ptr<Monotype>> FunMonos_;

  // track integer-literal-origin type variables (so we can default them later)
  std::vector<TypeVar> IntLiteralVars_;
  std::vector<TypeVar> FloatLiteralVars_;

  // expected return type stack
  std::vector<std::shared_ptr<Monotype>> CurrentFnReturnTy_;

  using InferRes = std::pair<Substitution, std::shared_ptr<Monotype>>;

  // main passes
  void predeclare();
  void inferDecl(Decl &D);
  void inferVarDecl(VarDecl &D);
  void inferFunDecl(FunDecl &D);

  // expr inference (same declarations as before) ...
  // expr inference (same declarations as before) ...
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
  void annotateExpr(Expr &E, const std::shared_ptr<Monotype> &T);

  // record and propagate substitutions into global state + env
  void recordSubst(const Substitution &S);

  // integer defaulting (must compose into GlobalSubst_ and env)
  void defaultIntegerLiterals();
  void defaultFloatLiterals();

  // After inference & defaulting, finalize by applying GlobalSubst_
  // to stored Monotypes and writing concrete phi::Type back into AST.
  void finalizeAnnotations();
  struct IntConstraint {
    TypeVar Var;
    SrcLocation Loc;
  };
  std::vector<IntConstraint> IntRangeVars_;

  bool isIntegerType(const std::shared_ptr<Monotype> &T) const {
    if (T->tag() != Monotype::Tag::Con)
      return false;
    auto name = T->conName();
    return name == "i8" || name == "i16" || name == "i32" || name == "i64";
  }

  // Helper function to check if a type variable comes from a float literal
  bool isFloatLiteralVar(const std::shared_ptr<Monotype> &t) const {
    if (t->tag() != Monotype::Tag::Var)
      return false;
    auto var = t->asVar();
    return std::find(FloatLiteralVars_.begin(), FloatLiteralVars_.end(), var) !=
           FloatLiteralVars_.end();
  }

  // Helper function to check if a type variable comes from an int literal
  bool isIntLiteralVar(const std::shared_ptr<Monotype> &t) const {
    if (t->tag() != Monotype::Tag::Var)
      return false;
    auto var = t->asVar();
    return std::find(IntLiteralVars_.begin(), IntLiteralVars_.end(), var) !=
           IntLiteralVars_.end();
  }

  std::string monotypeToString(const std::shared_ptr<Monotype> &T) const {
    // Simple monotype to string conversion
    if (T->tag() == Monotype::Tag::Con)
      return T->conName();
    if (T->tag() == Monotype::Tag::Var)
      return "type_var";
    return "unknown_type";
  }

  // Add new method:
  void checkIntegerConstraints() {
    for (const auto &constraint : IntRangeVars_) {
      auto T = GlobalSubst_.apply(Monotype::var(constraint.Var));
      if (!isIntegerType(T)) {
        std::string msg = std::format(
            "Loop variable must be integer type, got: {} at location {}:{}",
            monotypeToString(T), constraint.Loc.line, constraint.Loc.col);

        throw std::runtime_error(msg);
      }
    }
  }

  // token-kind helpers (same as before)
  bool isArithmetic(TokenKind K) const noexcept;
  bool isLogical(TokenKind K) const noexcept;
  bool isComparison(TokenKind K) const noexcept;
  bool isEquality(TokenKind K) const noexcept;
};

} // namespace phi
