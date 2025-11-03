#pragma once

#include <memory>
#include <optional>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"
#include "Sema/TypeInference/TypeVarFactory.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// TypeInferencer - Hindley-Milner type inference for Phi AST
//===----------------------------------------------------------------------===//

class TypeInferencer {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  TypeInferencer(std::vector<std::unique_ptr<Decl>> Ast,
                 std::shared_ptr<DiagnosticManager> DiagMan);

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::vector<std::unique_ptr<Decl>> inferProgram();

  //===--------------------------------------------------------------------===//
  // Declaration Visitor Methods
  //===--------------------------------------------------------------------===//

  void visit(Decl &D);
  void visit(VarDecl &D);
  void visit(ParamDecl &D);
  void visit(FunDecl &D);
  void visit(FieldDecl &D);
  void visit(MethodDecl &D);
  void visit(StructDecl &D);
  void visit(EnumDecl &D);

  //===--------------------------------------------------------------------===//
  // Inference Result Type Definition
  //===--------------------------------------------------------------------===//

  using InferRes = std::pair<Substitution, Monotype>;

  //===--------------------------------------------------------------------===//
  // Statement Visitor Methods -> return InferRes
  //===--------------------------------------------------------------------===//

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

  InferRes visit(Block &B);

  //===--------------------------------------------------------------------===//
  // Expression Visitor Methods -> return InferRes
  //===--------------------------------------------------------------------===//

  InferRes visit(Expr &E);
  InferRes visit(IntLiteral &E);
  InferRes visit(FloatLiteral &E);
  InferRes visit(BoolLiteral &E);
  InferRes visit(CharLiteral &E);
  InferRes visit(StrLiteral &E);
  InferRes visit(RangeLiteral &E);
  InferRes visit(TupleLiteral &E);
  InferRes visit(DeclRefExpr &E);
  InferRes visit(FunCallExpr &E);
  InferRes visit(BinaryOp &E);
  InferRes visit(UnaryOp &E);
  InferRes visit(StructLiteral &E);
  InferRes visit(FieldInitExpr &E);
  InferRes visit(FieldAccessExpr &E);
  InferRes visit(MethodCallExpr &E);

private:
  std::shared_ptr<DiagnosticManager> DiagMan;

  //===--------------------------------------------------------------------===//
  // Core Inference State
  //===--------------------------------------------------------------------===//

  std::vector<std::unique_ptr<Decl>> Ast;
  TypeEnv Env;
  TypeVarFactory Factory;

  std::unordered_map<std::string, StructDecl *> Structs;

  // Accumulate all substitutions produced during inference so we can finalize.
  Substitution GlobalSubst;

  //===--------------------------------------------------------------------===//
  // Type Annotation Side Tables
  //===--------------------------------------------------------------------===//

  // Side tables: store the HM monotypes for nodes until finalization.
  std::unordered_map<Expr *, Monotype> ExprMonos;
  std::unordered_map<ValueDecl *, Monotype> ValDeclMonos;
  std::unordered_map<FunDecl *, Monotype> FunDeclMonos;

  //===--------------------------------------------------------------------===//
  // Numeric Type Variable Tracking
  //===--------------------------------------------------------------------===//

  std::vector<TypeVar> IntTypeVars;
  std::vector<TypeVar> FloatTypeVars;

  //===--------------------------------------------------------------------===//
  // Function Context Stack
  //===--------------------------------------------------------------------===//

  // expected return type stack
  std::vector<Monotype> CurFunRetType;

  //===--------------------------------------------------------------------===//
  // Main Inference Passes
  //===--------------------------------------------------------------------===//

  void predeclare();

  //===--------------------------------------------------------------------===//
  // Unification Utilities
  //===--------------------------------------------------------------------===//

  void unifyInto(Substitution &S, const Monotype &A, const Monotype &B);
  Substitution unify(const Monotype &A, const Monotype &B);
  Substitution unifyVar(const Monotype &Var, const Monotype &B);
  Substitution unifyCon(const Monotype &A, const Monotype &B);
  Substitution unifyApp(const Monotype &A, const Monotype &B);
  Substitution unifyFun(const Monotype &A, const Monotype &B);
  void emitUnifyError(const Monotype &A, const Monotype &B,
                      const std::string &TopMsg,
                      const std::optional<std::string> &Note = std::nullopt);
  InferRes unifyAndAnnotate(Expr &E, Substitution S, const Monotype &ExprType,
                            const Monotype &ExpectedType);
  InferRes unifyAndAnnotate(Expr &E, Substitution S, const Monotype &Type1,
                            const Monotype &Type2,
                            const Monotype &ExpectedType);
  //===--------------------------------------------------------------------===//
  // Annotation Management
  //===--------------------------------------------------------------------===//

  void annotate(ValueDecl &D, const Monotype &T);
  void annotate(Expr &E, const Monotype &T);

  // record and propagate substitutions into global state + env
  void recordSubst(const Substitution &S);

  //===--------------------------------------------------------------------===//
  // Type Defaulting & Finalization
  //===--------------------------------------------------------------------===//

  void defaultNums();

  // After inference & defaulting, finalize by applying GlobalSubst
  // to stored Monotypes and writing concrete phi::Type back into AST.
  void finalizeAnnotations();

  std::tuple<Substitution, Monotype, StructDecl *>
  inferStructBase(Expr &BaseExpr, SrcLocation Loc);
};

} // namespace phi
