#pragma once

#include <memory>
#include <string>
#include <variant>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/TypeInference/Unifier.hpp"

namespace phi {

class TypeInferencer {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  TypeInferencer(std::vector<ModuleDecl *> Modules, DiagnosticManager *DiagMan)
      : Modules(std::move(Modules)), DiagMan(DiagMan) {}

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::vector<ModuleDecl *> infer();

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
  void visit(VariantDecl &D);
  void visit(ModuleDecl &D);
  TypeRef instantiate(Decl *D);
  std::unordered_map<const TypeArgDecl *, TypeRef> buildGenericSubstMap(
      const std::vector<std::unique_ptr<TypeArgDecl>> &TypeArgs);
  TypeRef substituteGenerics(
      TypeRef Ty, const std::unordered_map<const TypeArgDecl *, TypeRef> &Map);

  //===--------------------------------------------------------------------===//
  // Statement Visitor Methods
  //===--------------------------------------------------------------------===//

  void visit(Stmt &S);
  void visit(ReturnStmt &S);
  void visit(DeferStmt &S);
  void visit(ForStmt &S);
  void visit(WhileStmt &S);
  void visit(IfStmt &S);
  void visit(DeclStmt &S);
  void visit(BreakStmt &S);
  void visit(ContinueStmt &S);
  void visit(ExprStmt &S);
  void visit(Block &B);

  //===--------------------------------------------------------------------===//
  // Expression Visitor Methods
  //===--------------------------------------------------------------------===//

  TypeRef visit(Expr &E);
  TypeRef visit(IntLiteral &E);
  TypeRef visit(FloatLiteral &E);
  TypeRef visit(BoolLiteral &E);
  TypeRef visit(CharLiteral &E);
  TypeRef visit(StrLiteral &E);
  TypeRef visit(RangeLiteral &E);
  TypeRef visit(TupleLiteral &E);
  TypeRef visit(DeclRefExpr &E);
  TypeRef visit(FunCallExpr &E);
  TypeRef visit(BinaryOp &E);
  TypeRef visit(UnaryOp &E);
  TypeRef visit(AdtInit &E);
  TypeRef visit(MemberInit &E);
  TypeRef visit(FieldAccessExpr &E);
  TypeRef visit(MethodCallExpr &E);
  TypeRef visit(MatchExpr &E);
  TypeRef visit(IntrinsicCall &E);
  TypeRef visit(IndexExpr &E);

  std::string toString(TypeRef T);

private:
  std::vector<ModuleDecl *> Modules;
  DiagnosticManager *DiagMan;
  std::variant<FunDecl *, MethodDecl *, std::monostate> CurrentFun =
      std::monostate();

  TypeUnifier Unifier;

  //===--------------------------------------------------------------------===//
  // Declaration Finalize Methods
  //===--------------------------------------------------------------------===//

  void finalize(Decl &D);
  void finalize(VarDecl &D);
  void finalize(FunDecl &D);
  void finalize(MethodDecl &D);
  void finalize(StructDecl &D);
  void finalize(EnumDecl &D);
  void finalize(ModuleDecl &D);

  //===--------------------------------------------------------------------===//
  // Statement Finalize Methods
  //===--------------------------------------------------------------------===//

  void finalize(Stmt &S);
  void finalize(ReturnStmt &S);
  void finalize(DeferStmt &S);
  void finalize(ForStmt &S);
  void finalize(WhileStmt &S);
  void finalize(IfStmt &S);
  void finalize(DeclStmt &S);
  void finalize(BreakStmt &S);
  void finalize(ContinueStmt &S);
  void finalize(ExprStmt &S);
  void finalize(Block &B);

  //===--------------------------------------------------------------------===//
  // Expression Finalize Methods
  //===--------------------------------------------------------------------===//

  void finalize(Expr &E);
  void finalize(IntLiteral &E);
  void finalize(FloatLiteral &E);
  void finalize(BoolLiteral &E);
  void finalize(CharLiteral &E);
  void finalize(StrLiteral &E);
  void finalize(RangeLiteral &E);
  void finalize(TupleLiteral &E);
  void finalize(DeclRefExpr &E);
  void finalize(FunCallExpr &E);
  void finalize(BinaryOp &E);
  void finalize(UnaryOp &E);
  void finalize(AdtInit &E);
  void finalize(MemberInit &E);
  void finalize(FieldAccessExpr &E);
  void finalize(MethodCallExpr &E);
  void finalize(MatchExpr &E);
  void finalize(IntrinsicCall &E);
  void finalize(IndexExpr &E);

  std::optional<TypeRef> defaultVarTy(TypeRef);
};

} // namespace phi
