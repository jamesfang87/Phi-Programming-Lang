#pragma once

#include <memory>
#include <utility>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticManager.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// TypeChecker - Semantic analysis and type checking for Phi AST
//===----------------------------------------------------------------------===//

class TypeChecker {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  TypeChecker(std::vector<std::unique_ptr<Decl>> Ast,
              std::shared_ptr<DiagnosticManager> Diag)
      : Ast(std::move(Ast)), Diag(std::move(Diag)) {}

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::pair<bool, std::vector<std::unique_ptr<Decl>>> check();

  //===--------------------------------------------------------------------===//
  // Expression Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(Expr &E);
  bool visit(IntLiteral &E);
  bool visit(FloatLiteral &E);
  bool visit(StrLiteral &E);
  bool visit(CharLiteral &E);
  bool visit(BoolLiteral &E);
  bool visit(RangeLiteral &E);
  bool visit(DeclRefExpr &E);
  bool visit(FunCallExpr &E);
  bool visit(BinaryOp &E);
  bool visit(UnaryOp &E);
  bool visit(StructLiteral &E);
  bool visit(FieldInitExpr &E);
  bool visit(FieldAccessExpr &E);
  bool visit(MethodCallExpr &E);

  //===--------------------------------------------------------------------===//
  // Statement Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(Stmt &S);
  bool visit(ReturnStmt &S);
  bool visit(DeferStmt &S);
  bool visit(IfStmt &S);
  bool visit(WhileStmt &S);
  bool visit(ForStmt &S);
  bool visit(DeclStmt &S);
  bool visit(BreakStmt &S);
  bool visit(ContinueStmt &S);
  bool visit(ExprStmt &S);
  bool visit(Block &B);

  //===--------------------------------------------------------------------===//
  // Declaration Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(Decl &D);
  bool visit(FunDecl &D);
  bool visit(ParamDecl &D);
  bool visit(StructDecl &D);
  bool visit(FieldDecl &D);
  bool visit(MethodDecl &D);
  bool visit(VarDecl &D);
  bool visit(EnumDecl &D);
  bool visit(VariantDecl &D);

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::vector<std::unique_ptr<Decl>> Ast;
  std::shared_ptr<DiagnosticManager> Diag;
  FunDecl *CurrentFun = nullptr;
};

} // namespace phi
