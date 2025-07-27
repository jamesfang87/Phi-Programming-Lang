#pragma once

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
class Block;
class ReturnStmt;
class IfStmt;
class WhileStmt;
class ForStmt;
class LetStmt;

/**
 * @brief Abstract visitor interface for AST traversal
 *
 * Provides the base interface for implementing the Visitor pattern over the
 * Abstract Syntax Tree (AST). Contains pure virtual methods for each AST node
 * type, allowing custom operations during traversal. Implementers should
 * override these methods to define node-specific behavior.
 */
class ASTVisitor {
public:
    virtual ~ASTVisitor() = default;

    // EXPRESSION VISITORS

    /**
     * @brief Visit method for integer literals
     * @param expr Reference to IntLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(IntLiteral& expr) = 0;

    /**
     * @brief Visit method for float literals
     * @param expr Reference to FloatLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(FloatLiteral& expr) = 0;

    /**
     * @brief Visit method for string literals
     * @param expr Reference to StrLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(StrLiteral& expr) = 0;

    /**
     * @brief Visit method for character literals
     * @param expr Reference to CharLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(CharLiteral& expr) = 0;

    /**
     * @brief Visit method for boolean literals
     * @param expr Reference to BoolLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(BoolLiteral& expr) = 0;

    /**
     * @brief Visit method for range literals
     * @param expr Reference to RangeLiteral node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(RangeLiteral& expr) = 0;

    /**
     * @brief Visit method for declaration references
     * @param expr Reference to DeclRefExpr node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(DeclRefExpr& expr) = 0;

    /**
     * @brief Visit method for function calls
     * @param expr Reference to FunCallExpr node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(FunCallExpr& expr) = 0;

    /**
     * @brief Visit method for binary operations
     * @param expr Reference to BinaryOp node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(BinaryOp& expr) = 0;

    /**
     * @brief Visit method for unary operations
     * @param expr Reference to UnaryOp node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(UnaryOp& expr) = 0;

    // STATEMENT VISITORS

    /**
     * @brief Visit method for return statements
     * @param stmt Reference to ReturnStmt node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(ReturnStmt& stmt) = 0;

    /**
     * @brief Visit method for if statements
     * @param stmt Reference to IfStmt node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(IfStmt& stmt) = 0;

    /**
     * @brief Visit method for while loops
     * @param stmt Reference to WhileStmt node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(WhileStmt& stmt) = 0;

    /**
     * @brief Visit method for for loops
     * @param stmt Reference to ForStmt node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(ForStmt& stmt) = 0;

    /**
     * @brief Visit method for let statements
     * @param stmt Reference to LetStmt node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(LetStmt& stmt) = 0;

    /**
     * @brief Visit method for generic expressions
     * @param stmt Reference to Expr node
     * @return Boolean indicating whether to continue traversal
     */
    virtual bool visit(Expr& stmt) = 0;
};
