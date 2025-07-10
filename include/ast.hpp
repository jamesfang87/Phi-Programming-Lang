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
        : location(std::move(location)),
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
        : location(std::move(location)) {}

    virtual ~Stmt() = default;
    Stmt(const Stmt&) = delete;
    Stmt& operator=(const Decl&) = delete;

    virtual void info_dump(int level = 0) const = 0;

protected:
    SrcLocation location;
};

class Block {
public:
    Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(int level = 0) const;

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class ParamDecl : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier)),
          type(std::move(type)) {}

    void info_dump(int level = 0) const override;

private:
    Type type;
};

class FunctionDecl : public Decl {
public:
    FunctionDecl(SrcLocation location,
                 std::string identifier,
                 Type return_type,
                 std::unique_ptr<std::vector<std::unique_ptr<ParamDecl>>> params,
                 std::unique_ptr<Block> block_ptr)
        : Decl(std::move(location), std::move(identifier)),
          return_type(std::move(return_type)),
          params(std::move(params)),
          block(std::move(block_ptr)) {}

    void info_dump(int level = 0) const override;

private:
    Type return_type;
    std::unique_ptr<std::vector<std::unique_ptr<ParamDecl>>> params;
    std::unique_ptr<Block> block;
};

class Expr : public Stmt {
public:
    Expr(SrcLocation location)
        : Stmt(std::move(location)) {}
};

class ReturnStmt : public Stmt {
public:
    ReturnStmt(SrcLocation location, std::unique_ptr<Expr> expr)
        : Stmt(std::move(location)),
          expr(std::move(expr)) {}

    void info_dump(int level = 0) const override;

private:
    std::unique_ptr<Expr> expr;
};

class IntLiteral : public Expr {
public:
    IntLiteral(SrcLocation location, int64_t value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

private:
    int64_t value;
};

class FloatLiteral : public Expr {
public:
    FloatLiteral(SrcLocation location, double value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

private:
    double value;
};

class StrLiteral : public Expr {
public:
    StrLiteral(SrcLocation location, std::string value)
        : Expr(std::move(location)),
          value(std::move(value)) {}

    void info_dump(int level = 0) const override;

private:
    std::string value;
};

class CharLiteral : public Expr {
public:
    CharLiteral(SrcLocation location, char value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

private:
    char value;
};

class DeclRefExpr : public Expr {
public:
    DeclRefExpr(SrcLocation location, std::string identifier)
        : Expr(std::move(location)),
          identifier(std::move(identifier)) {}

    void info_dump(int level = 0) const override;

private:
    std::string identifier;
};

class FunCallExpr : public Expr {
public:
    FunCallExpr(SrcLocation location,
                std::unique_ptr<Expr> callee,
                std::vector<std::unique_ptr<Expr>> args)
        : Expr(std::move(location)),
          callee(std::move(callee)),
          args(std::move(args)) {}

    void info_dump(int level = 0) const override;

private:
    std::unique_ptr<Expr> callee;
    std::vector<std::unique_ptr<Expr>> args;
};
