#pragma once

class Expr;
class IntLiteral;
class FloatLiteral;
class StrLiteral;
class CharLiteral;
class RangeLiteral;
class DeclRefExpr;
class FunCallExpr;
class BinaryOp;
class UnaryOp;
class Block;
class ReturnStmt;
class IfStmt;
class WhileStmt;
class ForStmt;

class ASTVisitor {
public:
    virtual ~ASTVisitor() = default;

    // Expression visitors
    virtual bool visit(IntLiteral& expr) = 0;
    virtual bool visit(FloatLiteral& expr) = 0;
    virtual bool visit(StrLiteral& expr) = 0;
    virtual bool visit(CharLiteral& expr) = 0;
    virtual bool visit(RangeLiteral& expr) = 0;
    virtual bool visit(DeclRefExpr& expr) = 0;
    virtual bool visit(FunCallExpr& expr) = 0;
    virtual bool visit(BinaryOp& expr) = 0;
    virtual bool visit(UnaryOp& expr) = 0;

    // Statement visitors
    virtual bool visit(Block& block) = 0;
    virtual bool visit(ReturnStmt& stmt) = 0;
    virtual bool visit(IfStmt& stmt) = 0;
    virtual bool visit(WhileStmt& stmt) = 0;
    virtual bool visit(ForStmt& stmt) = 0;
    virtual bool visit(Expr& stmt) = 0;
};
