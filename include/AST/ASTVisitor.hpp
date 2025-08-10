#pragma once

#include "AST/Expr.hpp"
namespace phi {

// Forward declarations of AST node types
class Expr;
class IntLiteral;
class FloatLiteral;
class StrLiteral;
class CharLiteral;
class BoolLiteral;
class RangeLiteral;
class DeclRefExpr;
class FunCallExpr;
class BinaryOp;
class UnaryOp;
class StructInitExpr;
class FieldInitExpr;
class MemberAccessExpr;
class MemberFunCallExpr;

class Block;
class ReturnStmt;
class IfStmt;
class WhileStmt;
class ForStmt;
class DeclStmt;
class BreakStmt;
class ContinueStmt;

/**
 * @brief Abstract visitor interface for AST traversal
 */
template <typename ReturnType> class ASTVisitor {
public:
  virtual ~ASTVisitor() = default;

  // EXPRESSION VISITORS
  virtual ReturnType visit(IntLiteral &Expression) = 0;
  virtual ReturnType visit(FloatLiteral &Expression) = 0;
  virtual ReturnType visit(StrLiteral &Expression) = 0;
  virtual ReturnType visit(CharLiteral &Expression) = 0;
  virtual ReturnType visit(BoolLiteral &Expression) = 0;
  virtual ReturnType visit(RangeLiteral &Expression) = 0;
  virtual ReturnType visit(DeclRefExpr &Expression) = 0;
  virtual ReturnType visit(FunCallExpr &Expression) = 0;
  virtual ReturnType visit(BinaryOp &Expression) = 0;
  virtual ReturnType visit(UnaryOp &Expression) = 0;
  virtual ReturnType visit(StructInitExpr &Expression) = 0;
  virtual ReturnType visit(FieldInitExpr &Expression) = 0;
  virtual ReturnType visit(MemberAccessExpr &Expression) = 0;
  virtual ReturnType visit(MemberFunCallExpr &Expression) = 0;

  // STATEMENT VISITORS
  virtual ReturnType visit(ReturnStmt &Statement) = 0;
  virtual ReturnType visit(IfStmt &Statement) = 0;
  virtual ReturnType visit(WhileStmt &Statement) = 0;
  virtual ReturnType visit(ForStmt &Statement) = 0;
  virtual ReturnType visit(DeclStmt &Statement) = 0;
  virtual ReturnType visit(BreakStmt &Statement) = 0;
  virtual ReturnType visit(ContinueStmt &Statement) = 0;
  virtual ReturnType visit(Expr &Statement) = 0;
};

} // namespace phi
