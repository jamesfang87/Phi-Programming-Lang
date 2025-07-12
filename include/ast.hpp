#include "AST/Type.hpp"
#include "SrcLocation.hpp"
#include <cassert>
#include <cstdint>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#pragma once
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
