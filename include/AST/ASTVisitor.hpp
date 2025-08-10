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
  virtual ReturnType visit(IntLiteral &expr) = 0;
  virtual ReturnType visit(FloatLiteral &expr) = 0;
  virtual ReturnType visit(StrLiteral &expr) = 0;
  virtual ReturnType visit(CharLiteral &expr) = 0;
  virtual ReturnType visit(BoolLiteral &expr) = 0;
  virtual ReturnType visit(RangeLiteral &expr) = 0;
  virtual ReturnType visit(DeclRefExpr &expr) = 0;
  virtual ReturnType visit(FunCallExpr &expr) = 0;
  virtual ReturnType visit(BinaryOp &expr) = 0;
  virtual ReturnType visit(UnaryOp &expr) = 0;
  virtual ReturnType visit(StructInitExpr &expr) = 0;
  virtual ReturnType visit(FieldInitExpr &expr) = 0;
  virtual ReturnType visit(MemberAccessExpr &expr) = 0;
  virtual ReturnType visit(MemberFunCallExpr &expr) = 0;

  // STATEMENT VISITORS
  virtual ReturnType visit(ReturnStmt &stmt) = 0;
  virtual ReturnType visit(IfStmt &stmt) = 0;
  virtual ReturnType visit(WhileStmt &stmt) = 0;
  virtual ReturnType visit(ForStmt &stmt) = 0;
  virtual ReturnType visit(DeclStmt &stmt) = 0;
  virtual ReturnType visit(BreakStmt &stmt) = 0;
  virtual ReturnType visit(ContinueStmt &stmt) = 0;
  virtual ReturnType visit(Expr &stmt) = 0;
};

} // namespace phi
