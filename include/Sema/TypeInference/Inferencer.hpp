#pragma once

#include <expected>
#include <memory>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/TypeInference/Substitution.hpp"

namespace phi {

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

  std::vector<std::unique_ptr<Decl>> infer();

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

  using InferRes = std::pair<Substitution, TypeRef>;

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
  InferRes visit(CustomTypeCtor &E);
  InferRes visit(MemberInitExpr &E);
  InferRes visit(FieldAccessExpr &E);
  InferRes visit(MethodCallExpr &E);
  InferRes visit(MatchExpr &E);

private:
  std::expected<Substitution, Diagnostic> unify(TypeRef A, TypeRef B);
};

} // namespace phi
