#include "AST/Stmt.hpp"
#include "SrcLocation.hpp"
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#pragma once

class Expr : public Stmt {
public:
    Expr(SrcLocation location)
        : Stmt(std::move(location)) {}
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

class DeclRefExpr : public Expr {
public:
    DeclRefExpr(SrcLocation location, std::string identifier)
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
