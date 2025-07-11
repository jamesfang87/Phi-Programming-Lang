#include "ast-util.hpp"
#include <cassert>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#pragma once
class Stmt {
public:
    Stmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~Stmt() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] const SrcLocation& get_location() const { return location; }

protected:
    SrcLocation location;
};

class Block {
public:
    Block(std::vector<std::unique_ptr<Stmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(int level = 0) const;

    [[nodiscard]] const std::vector<std::unique_ptr<Stmt>>& get_stmts() const { return stmts; }

private:
    std::vector<std::unique_ptr<Stmt>> stmts;
};

class Decl {
public:
    Decl(SrcLocation location, std::string identifier)
        : location(std::move(location)),
          identifier(std::move(identifier)) {}

    virtual ~Decl() = default;

    virtual void info_dump(int level = 0) const = 0;

    [[nodiscard]] std::string get_id() const { return identifier; }
    [[nodiscard]] SrcLocation get_location() const { return location; }

protected:
    SrcLocation location;
    std::string identifier;
};

class ParamDecl : public Decl {
public:
    ParamDecl(SrcLocation location, std::string identifier, Type type)
        : Decl(std::move(location), std::move(identifier)),
          type(std::move(type)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const Type& get_type() const { return type; }

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

    [[nodiscard]] const Type& get_return_type() const { return return_type; }
    [[nodiscard]] const std::vector<std::unique_ptr<ParamDecl>>* get_params() const {
        return params.get();
    }
    [[nodiscard]] const Block* get_block() const { return block.get(); }

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

    [[nodiscard]] const Expr* get_expr() const { return expr.get(); }

private:
    std::unique_ptr<Expr> expr;
};

class IntLiteral : public Expr {
public:
    IntLiteral(SrcLocation location, int64_t value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] int64_t get_value() const { return value; }

private:
    int64_t value;
};

class FloatLiteral : public Expr {
public:
    FloatLiteral(SrcLocation location, double value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] double get_value() const { return value; }

private:
    double value;
};

class StrLiteral : public Expr {
public:
    StrLiteral(SrcLocation location, std::string value)
        : Expr(std::move(location)),
          value(std::move(value)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const std::string& get_value() const { return value; }

private:
    std::string value;
};

class CharLiteral : public Expr {
public:
    CharLiteral(SrcLocation location, char value)
        : Expr(std::move(location)),
          value(value) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] char get_value() const { return value; }

private:
    char value;
};

class DeclRef : public Expr {
public:
    DeclRef(SrcLocation location, std::string identifier)
        : Expr(std::move(location)),
          identifier(std::move(identifier)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const std::string& get_id() const { return identifier; }

private:
    std::string identifier;
};

class FunctionCall : public Expr {
public:
    FunctionCall(SrcLocation location,
                 std::unique_ptr<Expr> callee,
                 std::vector<std::unique_ptr<Expr>> args)
        : Expr(std::move(location)),
          callee(std::move(callee)),
          args(std::move(args)) {}

    void info_dump(int level = 0) const override;

    [[nodiscard]] const Expr* get_callee() const { return callee.get(); }
    [[nodiscard]] const std::vector<std::unique_ptr<Expr>>& get_args() const { return args; }

private:
    std::unique_ptr<Expr> callee;
    std::vector<std::unique_ptr<Expr>> args;
};

class ResolvedStmt {
public:
    ResolvedStmt(SrcLocation location)
        : location(std::move(location)) {}

    virtual ~ResolvedStmt() = default;

    virtual void info_dump(size_t level = 0) const = 0;

    [[nodiscard]] const SrcLocation& get_location() const { return location; }

private:
    SrcLocation location;
};

class ResolvedDecl {
public:
    ResolvedDecl(SrcLocation location, std::string identifier, Type type)
        : location(std::move(location)),
          identifier(std::move(identifier)),
          type(std::move(type)) {}

    virtual ~ResolvedDecl() = default;
    virtual void info_dump(size_t level = 0) const = 0;

    [[nodiscard]] std::string get_id() const { return identifier; }
    [[nodiscard]] SrcLocation get_location() const { return location; }
    [[nodiscard]] const Type& get_type() const { return type; }

protected:
    SrcLocation location;
    std::string identifier;
    Type type;
};

class ResolvedParamDecl : public ResolvedDecl {
public:
    ResolvedParamDecl(SrcLocation location, std::string identifier, Type type)
        : ResolvedDecl(std::move(location), std::move(identifier), std::move(type)) {}

    void info_dump(size_t level = 0) const override;

private:
};

class ResolvedBlock {
public:
    ResolvedBlock(std::vector<std::unique_ptr<ResolvedStmt>> stmts)
        : stmts(std::move(stmts)) {}

    void info_dump(size_t level = 0) const;

    [[nodiscard]] const std::vector<std::unique_ptr<ResolvedStmt>>& get_stmts() const {
        return stmts;
    }

private:
    std::vector<std::unique_ptr<ResolvedStmt>> stmts;
};

class ResolvedFunDecl : public ResolvedDecl {
public:
    ResolvedFunDecl(SrcLocation location,
                    std::string identifier,
                    Type type,
                    std::vector<std::unique_ptr<ResolvedParamDecl>> params,
                    std::unique_ptr<ResolvedBlock> body)
        : ResolvedDecl(std::move(location), std::move(identifier), std::move(type)),
          params(std::move(params)),
          body(std::move(body)) {}

    void info_dump(size_t level = 0) const override;

    [[nodiscard]] const std::vector<std::unique_ptr<ResolvedParamDecl>>& get_params() const {
        return params;
    }
    [[nodiscard]] const ResolvedBlock* get_body() const { return body.get(); }

    void set_block(std::unique_ptr<ResolvedBlock> block) { body = std::move(block); }

private:
    std::vector<std::unique_ptr<ResolvedParamDecl>> params;
    std::unique_ptr<ResolvedBlock> body;
};

class ResolvedExpr : public ResolvedStmt {
public:
    ResolvedExpr(SrcLocation location, Type type)
        : ResolvedStmt(std::move(location)),
          type(std::move(type)) {}

    void info_dump(size_t level = 0) const override;

    [[nodiscard]] const Type get_type() const { return type; }

private:
    Type type;
};

class ResolvedIntLiteral : public ResolvedExpr {
public:
    ResolvedIntLiteral(SrcLocation location, int64_t value)
        : ResolvedExpr(std::move(location), Type(Type::Primitive::i64)),
          value(value) {}

    void info_dump(size_t level = 0) const override;

private:
    int64_t value;
};

class ResolvedFloatLiteral : public ResolvedExpr {
public:
    ResolvedFloatLiteral(SrcLocation location, double value)
        : ResolvedExpr(std::move(location), Type(Type::Primitive::f64)),
          value(value) {}

    void info_dump(size_t level = 0) const override;

private:
    double value;
};

// TODO add other resolved literals

class ResolvedDeclRef : public ResolvedExpr {
public:
    ResolvedDeclRef(SrcLocation location,
                    Type type,
                    std::string identifier,
                    const ResolvedDecl* decl)
        : ResolvedExpr(std::move(location), std::move(type)),
          decl(decl),
          identifier(std::move(identifier)) {}

    [[nodiscard]] const ResolvedDecl* get_decl() const { return decl; }

    void info_dump(size_t level = 0) const override;

private:
    const ResolvedDecl* decl;
    std::string identifier;
};

class ResolvedFunctionCall : public ResolvedExpr {
public:
    ResolvedFunctionCall(SrcLocation location,
                         Type type,
                         ResolvedFunDecl* callee,
                         std::vector<std::unique_ptr<ResolvedExpr>> args)
        : ResolvedExpr(std::move(location), std::move(type)),
          callee(callee),
          args(std::move(args)) {}

    void info_dump(size_t level = 0) const override;

private:
    ResolvedFunDecl* callee;
    std::vector<std::unique_ptr<ResolvedExpr>> args;
};

class ResolvedReturnStmt : public ResolvedStmt {
public:
    ResolvedReturnStmt(SrcLocation location, std::unique_ptr<ResolvedExpr> expr)
        : ResolvedStmt(std::move(location)),
          expr(std::move(expr)) {}

    void info_dump(size_t level = 0) const override;

private:
    std::unique_ptr<ResolvedExpr> expr;
};
