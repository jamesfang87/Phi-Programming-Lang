#pragma once

#include <memory>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "SrcManager/SrcLocation.hpp"

class Decl;
class FunDecl;
class VarDecl;

class Stmt {
public:
    explicit Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    [[nodiscard]] SrcLocation& get_location() { return location; }

    virtual void info_dump(int level) const = 0;
    virtual bool accept(ASTVisitor& visitor) = 0;

protected:
    SrcLocation location;
};

class Block {
public:
    explicit Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    [[nodiscard]] std::vector<std::unique_ptr<Stmt>>& get_stmts() { return stmts; }

    void info_dump(int level) const;

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class ReturnStmt final : public Stmt {
public:
    ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr);
    ~ReturnStmt() override;

    [[nodiscard]] bool has_expr() const { return expr != nullptr; }
    [[nodiscard]] Expr& get_expr() const { return *expr; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> expr;
};

class IfStmt final : public Stmt {
public:
    IfStmt(SrcLocation location,
           std::unique_ptr<Expr> condition,
           std::unique_ptr<Block> then_block,
           std::unique_ptr<Block> else_block);
    ~IfStmt() override;

    [[nodiscard]] Expr& get_condition() const { return *condition; }
    [[nodiscard]] Block& get_then() const { return *then_block; }
    [[nodiscard]] Block& get_else() const { return *else_block; }
    [[nodiscard]] bool has_else() const { return else_block != nullptr; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> then_block;
    std::unique_ptr<Block> else_block;
};

class WhileStmt final : public Stmt {
public:
    WhileStmt(SrcLocation location, std::unique_ptr<Expr> condition, std::unique_ptr<Block> body);
    ~WhileStmt() override;

    [[nodiscard]] Expr& get_condition() const { return *condition; }
    [[nodiscard]] Block& get_body() const { return *body; };

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> body;
};

class ForStmt final : public Stmt {
public:
    ForStmt(SrcLocation location,
            std::unique_ptr<VarDecl> loop_var,
            std::unique_ptr<Expr> range,
            std::unique_ptr<Block> body);
    ~ForStmt() override;

    [[nodiscard]] VarDecl& get_loop_var() { return *loop_var; }
    [[nodiscard]] Expr& get_range() const { return *range; }
    [[nodiscard]] Block& get_body() const { return *body; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<VarDecl> loop_var;
    std::unique_ptr<Expr> range;
    std::unique_ptr<Block> body;
};

class LetStmt final : public Stmt {
public:
    LetStmt(SrcLocation location, std::unique_ptr<VarDecl> decl);
    ~LetStmt() override;

    [[nodiscard]] VarDecl& get_var_decl() const { return *var_decl; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<VarDecl> var_decl;
};
