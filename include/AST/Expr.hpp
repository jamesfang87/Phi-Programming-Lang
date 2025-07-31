#pragma once

#include <memory>
#include <optional>
#include <string>
#include <vector>

#include "AST/Stmt.hpp"
#include "AST/Type.hpp"
#include "Lexer/Token.hpp"
#include "Lexer/TokenType.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

/**
 * @brief Base class for all expression nodes
 *
 * Represents expressions that produce values during evaluation. Contains
 * optional type information that gets resolved during semantic analysis.
 */
class Expr : public Stmt {
public:
    /**
     * @brief Constructs expression
     * @param location Source location
     * @param type Optional type information
     */
    explicit Expr(SrcLocation location, std::optional<Type> type = std::nullopt)
        : Stmt(std::move(location)),
          type(std::move(type)) {}

    /**
     * @brief Retrieves expression type
     * @return Type object
     */
    [[nodiscard]] Type get_type() { return type.value(); }

    /**
     * @brief Checks if type has been resolved
     * @return true if type resolved, false otherwise
     */
    [[nodiscard]] bool is_resolved() const { return type.has_value(); }

    /**
     * @brief Sets expression type
     * @param t New type value
     */
    void set_type(Type t) { type = std::move(t); }

    /**
     * @brief Accepts AST visitor
     * @param visitor Visitor implementation
     * @return Visitor control result
     */
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

protected:
    std::optional<Type> type; ///< Resolved type (after semantic analysis)
};

/**
 * @brief Integer literal expression
 */
class IntLiteral final : public Expr {
public:
    /**
     * @brief Constructs integer literal
     * @param location Source location
     * @param value Integer value
     */
    IntLiteral(SrcLocation location, const int64_t value);

    /**
     * @brief Retrieves literal value
     * @return Integer value
     */
    [[nodiscard]] int64_t get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    int64_t value; ///< Literal integer value
};

/**
 * @brief Floating-point literal expression
 */
class FloatLiteral final : public Expr {
public:
    /**
     * @brief Constructs float literal
     * @param location Source location
     * @param value Float value
     */
    FloatLiteral(SrcLocation location, const double value);

    /**
     * @brief Retrieves literal value
     * @return Float value
     */
    [[nodiscard]] double get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    double value; ///< Literal float value
};

/**
 * @brief String literal expression
 */
class StrLiteral final : public Expr {
public:
    /**
     * @brief Constructs string literal
     * @param location Source location
     * @param value String value
     */
    StrLiteral(SrcLocation location, std::string value);

    /**
     * @brief Retrieves string value
     * @return String content
     */
    [[nodiscard]] std::string get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string value; ///< Literal string content
};

/**
 * @brief Character literal expression
 */
class CharLiteral final : public Expr {
public:
    /**
     * @brief Constructs char literal
     * @param location Source location
     * @param value Character value
     */
    CharLiteral(SrcLocation location, const char value);

    /**
     * @brief Retrieves character value
     * @return Character value
     */
    [[nodiscard]] char get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    char value; ///< Literal character value
};

/**
 * @brief Boolean literal expression
 */
class BoolLiteral final : public Expr {
public:
    /**
     * @brief Constructs boolean literal
     * @param location Source location
     * @param value Boolean value
     */
    BoolLiteral(SrcLocation location, const bool value);

    /**
     * @brief Retrieves boolean value
     * @return true or false
     */
    [[nodiscard]] bool get_value() const { return value; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    bool value; ///< Literal boolean value
};

/**
 * @brief Range literal expression (e.g., 1..10)
 */
class RangeLiteral final : public Expr {
public:
    /**
     * @brief Constructs range literal
     * @param location Source location
     * @param start Start expression
     * @param end End expression
     * @param inclusive Inclusive range flag
     */
    RangeLiteral(SrcLocation location,
                 std::unique_ptr<Expr> start,
                 std::unique_ptr<Expr> end,
                 const bool inclusive);

    /**
     * @brief Retrieves start expression
     * @return Reference to start expression
     */
    [[nodiscard]] Expr& get_start() { return *start; }

    /**
     * @brief Retrieves end expression
     * @return Reference to end expression
     */
    [[nodiscard]] Expr& get_end() { return *end; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> start, end; ///< Range bounds
    bool inclusive;                   ///< Inclusive range flag
};

/**
 * @brief Declaration reference expression
 *
 * Represents references to variables, functions, or other declarations.
 */
class DeclRefExpr final : public Expr {
public:
    /**
     * @brief Constructs declaration reference
     * @param location Source location
     * @param identifier Declaration name
     */
    DeclRefExpr(SrcLocation location, std::string identifier);

    /**
     * @brief Retrieves identifier
     * @return Identifier string
     */
    [[nodiscard]] std::string get_id() { return identifier; }

    /**
     * @brief Retrieves referenced declaration
     * @return Pointer to declaration (null if unresolved)
     */
    [[nodiscard]] Decl* get_decl() const { return decl; }

    /**
     * @brief Sets referenced declaration
     * @param d Declaration pointer
     */
    void set_decl(Decl* d) { decl = d; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::string identifier; ///< Referenced identifier
    Decl* decl = nullptr;   ///< Resolved declaration
};

/**
 * @brief Function call expression
 */
class FunCallExpr final : public Expr {
public:
    /**
     * @brief Constructs function call
     * @param location Source location
     * @param callee Callable expression
     * @param args Argument expressions
     */
    FunCallExpr(SrcLocation location,
                std::unique_ptr<Expr> callee,
                std::vector<std::unique_ptr<Expr>> args);

    /**
     * @brief Retrieves callee expression
     * @return Reference to callee expression
     */
    [[nodiscard]] Expr& get_callee() const { return *callee; }

    /**
     * @brief Retrieves arguments
     * @return Reference to argument list
     */
    [[nodiscard]] std::vector<std::unique_ptr<Expr>>& get_args() { return args; }

    /**
     * @brief Retrieves function declaration
     * @return Pointer to function declaration
     */
    [[nodiscard]] FunDecl* get_func_decl() const { return func_decl; }

    /**
     * @brief Sets function declaration
     * @param f Function declaration pointer
     */
    void set_func_decl(FunDecl* f) { func_decl = f; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> callee;            ///< Function being called
    std::vector<std::unique_ptr<Expr>> args; ///< Argument expressions
    FunDecl* func_decl = nullptr;            ///< Resolved function declaration
};

/**
 * @brief Binary operation expression
 */
class BinaryOp final : public Expr {
public:
    /**
     * @brief Constructs binary operation
     * @param lhs Left-hand expression
     * @param rhs Right-hand expression
     * @param op Operator token
     */
    BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs, const Token& op);

    /**
     * @brief Retrieves left operand
     * @return Reference to left expression
     */
    [[nodiscard]] Expr& get_lhs() const { return *lhs; }

    /**
     * @brief Retrieves right operand
     * @return Reference to right expression
     */
    [[nodiscard]] Expr& get_rhs() const { return *rhs; }

    /**
     * @brief Retrieves operator
     * @return Operator token type
     */
    [[nodiscard]] TokenType get_op() const { return op; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> lhs; ///< Left operand
    std::unique_ptr<Expr> rhs; ///< Right operand
    TokenType op;              ///< Operator token type
};

/**
 * @brief Unary operation expression
 */
class UnaryOp final : public Expr {
public:
    /**
     * @brief Constructs unary operation
     * @param operand Target expression
     * @param op Operator token
     * @param is_prefix Prefix position flag
     */
    UnaryOp(std::unique_ptr<Expr> operand, const Token& op, const bool is_prefix);

    /**
     * @brief Retrieves operand expression
     * @return Reference to operand
     */
    [[nodiscard]] Expr& get_operand() const { return *operand; }

    /**
     * @brief Retrieves operator
     * @return Operator token type
     */
    [[nodiscard]] TokenType get_op() const { return op; }

    /**
     * @brief Checks if operator is prefix
     * @return true if prefix, false if postfix
     */
    [[nodiscard]] bool is_prefix_op() const { return is_prefix; }

    void info_dump(int level) const override;
    bool accept(ASTVisitor& visitor) override { return visitor.visit(*this); }

private:
    std::unique_ptr<Expr> operand; ///< Target expression
    TokenType op;                  ///< Operator token type
    bool is_prefix;                ///< Prefix position flag
};

} // namespace phi
