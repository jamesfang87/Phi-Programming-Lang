#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenType.hpp"
#include "SrcLocation.hpp"

class Expr : public Stmt {
public:
    explicit Expr(SrcLocation location, std::optional<Type> type = std::nullopt)
        : Stmt(std::move(location)),
          type(std::move(type)) {}

    [[nodiscard]] Type get_type() { return type.value(); }
    [[nodiscard]] bool is_resolved() const { return type.has_value(); }

    void set_type(Type t) { type = std::move(t); }

    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

protected:
    std::optional<Type> type;
};

class IntLiteral final : public Expr {
public:
    IntLiteral(SrcLocation location, const int64_t value);

    [[nodiscard]] int64_t get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    int64_t value;
};

class FloatLiteral final : public Expr {
public:
    FloatLiteral(SrcLocation location, const double value);

    [[nodiscard]] double get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    double value;
};

class StrLiteral final : public Expr {
public:
    StrLiteral(SrcLocation location, std::string value);

    [[nodiscard]] const std::string& get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string value;
};

class CharLiteral final : public Expr {
public:
    CharLiteral(SrcLocation location, const char value);

    [[nodiscard]] char get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    char value;
};

class BoolLiteral final : public Expr {
public:
    BoolLiteral(SrcLocation location, const bool value);

    [[nodiscard]] bool get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    bool value;
};

class RangeLiteral final : public Expr {
public:
    RangeLiteral(SrcLocation location,
                 std::unique_ptr<Expr> start,
                 std::unique_ptr<Expr> end,
                 const bool inclusive);

    [[nodiscard]] Expr& get_start() { return *start; }
    [[nodiscard]] Expr& get_end() { return *end; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> start, end;
    bool inclusive;
};

class DeclRefExpr final : public Expr {
public:
    DeclRefExpr(SrcLocation location, std::string identifier);

    [[nodiscard]] std::string& get_id() { return identifier; }
    [[nodiscard]] Decl* get_decl() const { return decl; }

    void set_decl(Decl* d) { decl = d; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string identifier;
    Decl* decl = nullptr;
};

class FunCallExpr final : public Expr {
public:
    FunCallExpr(SrcLocation location,
                std::unique_ptr<Expr> callee,
                std::vector<std::unique_ptr<Expr>> args);

    [[nodiscard]] Expr& get_callee() const { return *callee; }
    [[nodiscard]] std::vector<std::unique_ptr<Expr>>& get_args() { return args; }
    [[nodiscard]] FunDecl* get_func_decl() const { return func_decl; }

    void set_func_decl(FunDecl* f) { func_decl = f; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> callee;
    std::vector<std::unique_ptr<Expr>> args;
    FunDecl* func_decl = nullptr; // Non-owning pointer
};

class BinaryOp final : public Expr {
public:
    BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs, const Token& op);

    [[nodiscard]] Expr& get_lhs() const { return *lhs; }
    [[nodiscard]] Expr& get_rhs() const { return *rhs; }
    [[nodiscard]] TokenType get_op() const { return op; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> lhs;
    std::unique_ptr<Expr> rhs;
    TokenType op;
};

class UnaryOp final : public Expr {
public:
    UnaryOp(std::unique_ptr<Expr> operand, const Token& op, const bool is_prefix);

    [[nodiscard]] Expr& get_operand() const { return *operand; }
    [[nodiscard]] TokenType get_op() const { return op; }
    [[nodiscard]] bool is_prefix_op() const { return is_prefix; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> operand;
    TokenType op;
    bool is_prefix;
};
