#include "ast-util.hpp"
#include <cassert>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#pragma once
class Decl {
public:
    Decl(SrcLocation location, std::string identifier)
        : location(location),
          identifier(std::move(identifier)) {}

    virtual ~Decl() = default;
    Decl(const Decl&) = delete;
    Decl& operator=(const Decl&) = delete;

    virtual void info_dump(int level = 0) const = 0;

protected:
    SrcLocation location;
    std::string identifier;
};

class Stmt {
public:
    Stmt(SrcLocation location)
        : location(location) {}

    virtual ~Stmt() = default;
    Stmt(const Stmt&) = delete;
    Stmt& operator=(const Decl&) = delete;

    virtual void info_dump(int level = 0) const = 0;

protected:
    SrcLocation location;
};

class Block {
public:
    Block(std::vector<std::unique_ptr<Stmt>> statements)
        : statements(std::move(statements)) {}

    void info_dump(int level = 0) const;

private:
    std::vector<std::unique_ptr<Stmt>> statements;
};

class FunctionDecl : public Decl {
public:
    FunctionDecl(SrcLocation location,
                 std::string identifier,
                 Type return_type,
                 std::unique_ptr<Block> block_ptr)
        : Decl(location, std::move(identifier)),
          return_type(std::move(return_type)),
          block(std::move(block_ptr)) {}

    void info_dump(int level = 0) const override;

private:
    Type return_type;
    std::unique_ptr<Block> block;
};

class Expr : public Stmt {
public:
    Expr(SrcLocation location)
          : Stmt(location) {}
};

class ReturnStmt : public Stmt {
public:
    ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
        : Stmt(location),
          expr(std::move(expr)) {}

    void info_dump(int level = 0) const override;
private:
    std::unique_ptr<Expr> expr;
};

class IntLiteral : public Expr {
public:
    IntLiteral(SrcLocation location, int64_t value)
        : Expr(location),
          value(value) {}

    void info_dump(int level = 0) const override;

private:
    int64_t value;
};

class FloatLiteral : public Expr {
public:
    FloatLiteral(SrcLocation location, double value)
        : Expr(location),
          value(value) {}

    void info_dump(int level = 0) const override;

private:
    double value;
};

class DeclRefExpr : public Expr {
public:
    DeclRefExpr(SrcLocation location, std::string identifier)
        : Expr(location),
          identifier(std::move(identifier)) {}

    void info_dump(int level = 0) const override;

private:
    std::string identifier;
};
