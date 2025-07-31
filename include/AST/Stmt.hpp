#pragma once

#include <memory>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

// Forward declarations
class Decl;
class FunDecl;
class VarDecl;

/**
 * @brief Base class for all statement nodes
 *
 * Represents executable constructs in the Phi language. Contains source location
 * information and defines the visitor acceptance interface.
 */
class Stmt {
public:
    explicit Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    /**
     * @brief Retrieves source location
     * @return Reference to source location
     */
    [[nodiscard]] SrcLocation& get_location() { return location; }

    /**
     * @brief Debug output method
     * @param level Indentation level
     */
    virtual void info_dump(int level) const = 0;

    /**
     * @brief Accepts AST visitor
     * @param visitor Visitor implementation
     * @return Visitor control result
     */
    virtual bool accept(ASTVisitor& visitor) = 0;

protected:
    SrcLocation location; ///< Source location of statement
};

/**
 * @brief Block statement container
 *
 * Represents a sequence of statements enclosed in braces. Used in control flow
 * structures and function bodies.
 */
class Block {
public:
    /**
     * @brief Constructs block
     * @param stmts Contained statements
     */
    explicit Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    /**
     * @brief Retrieves contained statements
     * @return Reference to statement list
     */
    [[nodiscard]] std::vector<std::unique_ptr<Stmt>>& get_stmts() { return stmts; }

    /**
     * @brief Debug output for block
     * @param level Indentation level
     */
    void info_dump(int level) const;

private:
    std::vector<std::unique_ptr<Stmt>> stmts; ///< Contained statements
};

/**
 * @brief Return statement
 */
class ReturnStmt final : public Stmt {
public:
    /**
     * @brief Constructs return statement
     * @param location Source location
     * @param expr Return value expression
     */
    ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr);
    ~ReturnStmt() override;

    /**
     * @brief Checks if has return value
     * @return true if has expression, false otherwise
     */
    [[nodiscard]] bool has_expr() const { return expr != nullptr; }

    /**
     * @brief Retrieves return expression
     * @return Reference to return expression
     */
    [[nodiscard]] Expr& get_expr() const { return *expr; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> expr; ///< Return value expression
};

/**
 * @brief If conditional statement
 */
class IfStmt final : public Stmt {
public:
    /**
     * @brief Constructs if statement
     * @param location Source location
     * @param condition Conditional expression
     * @param then_block Then block
     * @param else_block Else block (optional)
     */
    IfStmt(SrcLocation location,
           std::unique_ptr<Expr> condition,
           std::unique_ptr<Block> then_block,
           std::unique_ptr<Block> else_block);
    ~IfStmt() override;

    /**
     * @brief Retrieves condition expression
     * @return Reference to condition
     */
    [[nodiscard]] Expr& get_condition() const { return *condition; }

    /**
     * @brief Retrieves then block
     * @return Reference to then block
     */
    [[nodiscard]] Block& get_then() const { return *then_block; }

    /**
     * @brief Retrieves else block
     * @return Reference to else block
     */
    [[nodiscard]] Block& get_else() const { return *else_block; }

    /**
     * @brief Checks if has else block
     * @return true if has else block, false otherwise
     */
    [[nodiscard]] bool has_else() const { return else_block != nullptr; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;   ///< Conditional expression
    std::unique_ptr<Block> then_block; ///< Then clause block
    std::unique_ptr<Block> else_block; ///< Else clause block (optional)
};

/**
 * @brief While loop statement
 */
class WhileStmt final : public Stmt {
public:
    /**
     * @brief Constructs while loop
     * @param location Source location
     * @param condition Loop condition
     * @param body Loop body block
     */
    WhileStmt(SrcLocation location, std::unique_ptr<Expr> condition, std::unique_ptr<Block> body);
    ~WhileStmt() override;

    /**
     * @brief Retrieves condition expression
     * @return Reference to condition
     */
    [[nodiscard]] Expr& get_condition() const { return *condition; }

    /**
     * @brief Retrieves loop body
     * @return Reference to body block
     */
    [[nodiscard]] Block& get_body() const { return *body; };

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition; ///< Loop condition
    std::unique_ptr<Block> body;     ///< Loop body block
};

/**
 * @brief For loop statement
 */
class ForStmt final : public Stmt {
public:
    /**
     * @brief Constructs for loop
     * @param location Source location
     * @param loop_var Loop variable declaration
     * @param range Range expression
     * @param body Loop body block
     */
    ForStmt(SrcLocation location,
            std::unique_ptr<VarDecl> loop_var,
            std::unique_ptr<Expr> range,
            std::unique_ptr<Block> body);
    ~ForStmt() override;

    /**
     * @brief Retrieves loop variable
     * @return Reference to loop variable declaration
     */
    [[nodiscard]] VarDecl& get_loop_var() { return *loop_var; }

    /**
     * @brief Retrieves range expression
     * @return Reference to range expression
     */
    [[nodiscard]] Expr& get_range() const { return *range; }

    /**
     * @brief Retrieves loop body
     * @return Reference to body block
     */
    [[nodiscard]] Block& get_body() const { return *body; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<VarDecl> loop_var; ///< Loop variable declaration
    std::unique_ptr<Expr> range;       ///< Range expression
    std::unique_ptr<Block> body;       ///< Loop body block
};

/**
 * @brief Variable declaration statement
 */
class LetStmt final : public Stmt {
public:
    /**
     * @brief Constructs let statement
     * @param location Source location
     * @param decl Variable declaration
     */
    LetStmt(SrcLocation location, std::unique_ptr<VarDecl> decl);
    ~LetStmt() override;

    /**
     * @brief Retrieves variable declaration
     * @return Reference to variable declaration
     */
    [[nodiscard]] VarDecl& get_var_decl() const { return *var_decl; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<VarDecl> var_decl; ///< Variable declaration
};

} // namespace phi
