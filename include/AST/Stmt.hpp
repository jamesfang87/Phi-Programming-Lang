#include "ASTVisitor.hpp"
#include "SrcLocation.hpp"
#include <memory>
#include <vector>

#pragma once

class Stmt {
public:
    Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] SrcLocation& get_location() { return location; }
    virtual bool accept(ASTVisitor& visitor) = 0;

protected:
    SrcLocation location;
};

class Block {
public:
    Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(int level = 0) const;

    [[nodiscard]] std::vector<std::unique_ptr<Stmt>>& get_stmts() { return stmts; }

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class ReturnStmt : public Stmt {
public:
    ReturnStmt(SrcLocation, std::unique_ptr<Expr>);
    ~ReturnStmt() override;

    [[nodiscard]] Expr* get_expr();
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> expr;
};

class IfStmt : public Stmt {
public:
    IfStmt(SrcLocation, std::unique_ptr<Expr>, std::unique_ptr<Block>, std::unique_ptr<Block>);
    ~IfStmt() override;

    [[nodiscard]] Expr& get_condition();
    [[nodiscard]] Block& get_true_body();
    [[nodiscard]] Block& get_false_body();
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> true_body;
    std::unique_ptr<Block> false_body;
};

class WhileStmt : public Stmt {
public:
    WhileStmt(SrcLocation, std::unique_ptr<Expr>, std::unique_ptr<Block>);
    ~WhileStmt() override;

    [[nodiscard]] Expr& get_condition();
    [[nodiscard]] Block& get_body();
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> condition;
    std::unique_ptr<Block> body;
};

class ForStmt : public Stmt {
public:
    ForStmt(SrcLocation, std::string loop_var, std::unique_ptr<Expr>, std::unique_ptr<Block>);
    ~ForStmt() override;

    [[nodiscard]] std::string& get_loop_var();
    [[nodiscard]] Expr& get_range();
    [[nodiscard]] Block& get_body();
    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string loop_var;
    std::unique_ptr<Expr> range;
    std::unique_ptr<Block> body;
};
