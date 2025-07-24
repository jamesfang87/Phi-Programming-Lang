#pragma once
#include "AST/Stmt.hpp"
#include "Lexer/Token.hpp"
#include "SrcLocation.hpp"
#include "Type.hpp"
#include <memory>
#include <optional>
#include <string>
#include <vector>

class Expr : public Stmt {
public:
    friend class Sema;

    explicit Expr(SrcLocation location, std::optional<Type> type = std::nullopt)
        : Stmt(std::move(location)),
          type(std::move(type)) {}

    [[nodiscard]] std::optional<Type>& get_type_opt() { return type; }
    [[nodiscard]] Type& get_type() {
        if (!type) throw std::logic_error("Type not resolved");
        return *type;
    }
    void set_type(Type t) { type = std::move(t); }
    [[nodiscard]] bool is_resolved() const { return type.has_value(); }

    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

protected:
    std::optional<Type> type;
};

class IntLiteral final : public Expr {
public:
    friend class Sema;

    IntLiteral(SrcLocation location, const int64_t value)
        : Expr(std::move(location), Type(Type::Primitive::i64)),
          value(value) {}

    void info_dump(int level) const override;

    [[nodiscard]] int64_t get_value() const { return value; }

    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    int64_t value;
};

class FloatLiteral final : public Expr {
public:
    friend class Sema;

    FloatLiteral(SrcLocation location, const double value)
        : Expr(std::move(location), Type(Type::Primitive::f64)),
          value(value) {}

    void info_dump(int level) const override;

    [[nodiscard]] double get_value() const { return value; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    double value;
};

class StrLiteral final : public Expr {
public:
    friend class Sema;

    StrLiteral(SrcLocation location, std::string value)
        : Expr(std::move(location), Type(Type::Primitive::str)),
          value(std::move(value)) {}

    void info_dump(int level) const override;

    [[nodiscard]] const std::string& get_value() const { return value; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string value;
};

class CharLiteral final : public Expr {
public:
    friend class Sema;

    CharLiteral(SrcLocation location, const char value)
        : Expr(std::move(location), Type(Type::Primitive::character)),
          value(value) {}

    void info_dump(int level) const override;

    [[nodiscard]] char get_value() const { return value; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    char value;
};

class BoolLiteral final : public Expr {
public:
    friend class Sema;

    BoolLiteral(SrcLocation location, const bool value)
        : Expr(std::move(location), Type(Type::Primitive::boolean)),
          value(value) {}

    void info_dump(int level) const override;

    [[nodiscard]] bool get_value() const { return value; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    bool value;
};

class RangeLiteral final : public Expr {
public:
    friend class Sema;

    RangeLiteral(SrcLocation location,
                 std::unique_ptr<Expr> start,
                 std::unique_ptr<Expr> end,
                 const bool inclusive)
        : Expr(std::move(location)),
          start(std::move(start)),
          end(std::move(end)),
          inclusive(inclusive) {}

    void info_dump(int level) const override;

    [[nodiscard]] std::unique_ptr<Expr>& get_start() { return start; }
    [[nodiscard]] std::unique_ptr<Expr>& get_end() { return end; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> start, end;
    bool inclusive;
};

class DeclRefExpr final : public Expr {
public:
    friend class Sema;

    DeclRefExpr(SrcLocation location, std::string identifier)
        : Expr(std::move(location)),
          identifier(std::move(identifier)) {}

    void info_dump(int level) const override;

    [[nodiscard]] std::string& get_id() { return identifier; }
    [[nodiscard]] Decl* get_decl() const { return decl; }
    void set_decl(Decl* d) { decl = d; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string identifier;
    Decl* decl = nullptr;
};

class FunCallExpr final : public Expr {
public:
    friend class Sema;

    FunCallExpr(SrcLocation location,
                std::unique_ptr<Expr> callee,
                std::vector<std::unique_ptr<Expr>> args)
        : Expr(std::move(location)),
          callee(std::move(callee)),
          args(std::move(args)) {}

    void info_dump(int level) const override;

    [[nodiscard]] Expr* get_callee() const { return callee.get(); }
    [[nodiscard]] std::vector<std::unique_ptr<Expr>>& get_args() { return args; }
    [[nodiscard]] FunDecl* get_func_decl() const { return func_decl; }
    void set_func_decl(FunDecl* f) { func_decl = f; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> callee;
    std::vector<std::unique_ptr<Expr>> args;
    FunDecl* func_decl = nullptr; // Non-owning pointer
};

class BinaryOp final : public Expr {
public:
    friend class Sema;

    BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs, const Token& op)
        : Expr(op.get_start()),
          lhs(std::move(lhs)),
          rhs(std::move(rhs)),
          op(op.get_type()) {}

    void info_dump(int level) const override;

    [[nodiscard]] Expr* get_lhs() const { return lhs.get(); }
    [[nodiscard]] Expr* get_rhs() const { return rhs.get(); }
    [[nodiscard]] TokenType get_op() const { return op; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> lhs;
    std::unique_ptr<Expr> rhs;
    TokenType op;
};

class UnaryOp final : public Expr {
public:
    friend class Sema;

    UnaryOp(std::unique_ptr<Expr> operand, const Token& op, const bool is_prefix)
        : Expr(op.get_start()),
          operand(std::move(operand)),
          op(op.get_type()),
          is_prefix(is_prefix) {}

    void info_dump(int level) const override;

    [[nodiscard]] Expr* get_operand() const { return operand.get(); }
    [[nodiscard]] TokenType get_op() const { return op; }
    [[nodiscard]] bool is_prefix_op() const { return is_prefix; }
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> operand;
    TokenType op;
    bool is_prefix;
};
