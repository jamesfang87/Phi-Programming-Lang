#pragma once

#include <memory>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "SrcLocation.hpp"

class Decl;
class FunDecl;
class VarDecl;

class Stmt {
public:
    explicit Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    virtual void info_dump(int level) const = 0;

    [[nodiscard]] SrcLocation& get_location() { return location; }
    virtual bool accept(ASTVisitor& visitor) = 0;

protected:
    SrcLocation location;
};

class Block {
public:
    explicit Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(int level) const;

    [[nodiscard]] std::vector<std::unique_ptr<Stmt>>& get_stmts() { return stmts; }
    bool accept(ASTVisitor& visitor) { return visitor.visit(*this); }

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class ReturnStmt final : public Stmt {
public:
    ReturnStmt(SrcLocation, std::unique_ptr<Expr>);
    ~ReturnStmt() override;

    [[nodiscard]] Expr* get_expr() const;
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> expr;
};

class IfStmt final : public Stmt {
public:
    friend class Sema;
    IfStmt(SrcLocation, std::unique_ptr<Expr>, std::unique_ptr<Block>, std::unique_ptr<Block>);
    ~IfStmt() override;

    [[nodiscard]] Expr& get_condition() const;
    [[nodiscard]] Block& get_true_body() const;
    [[nodiscard]] Block& get_false_body() const;
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> true_body;
    std::unique_ptr<Block> false_body;
};

class WhileStmt final : public Stmt {
public:
    friend class Sema;

    WhileStmt(SrcLocation, std::unique_ptr<Expr>, std::unique_ptr<Block>);
    ~WhileStmt() override;

    [[nodiscard]] Expr& get_condition() const;
    [[nodiscard]] Block& get_body() const;
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> body;
};

class ForStmt final : public Stmt {
public:
    ForStmt(SrcLocation, std::string, std::unique_ptr<Expr>, std::unique_ptr<Block>);
    ~ForStmt() override;

    [[nodiscard]] std::string& get_loop_var();
    [[nodiscard]] Expr& get_range() const;
    [[nodiscard]] Block& get_body() const;
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string loop_var;
    std::unique_ptr<Expr> range;
    std::unique_ptr<Block> body;
};

class LetStmt final : public Stmt {
public:
    friend class Sema;
    LetStmt(SrcLocation, std::unique_ptr<VarDecl>);
    ~LetStmt() override;

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<VarDecl> var_decl;
};
