#include "AST/Expr.hpp"

#include <cassert>
#include <print>
#include <string>

#include "Lexer/TokenType.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param level Current indentation level
/// @return String of spaces for indentation
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

//======================== IntLiteral Implementation ========================//

namespace phi {

/**
 * @brief Constructs an integer literal expression
 *
 * Default type is set to i64. The actual type may be
 * adjusted during semantic analysis based on value.
 *
 * @param location Source location of literal
 * @param value Integer value
 */
IntLiteral::IntLiteral(SrcLocation location, const int64_t value)
    : Expr(std::move(location), Type(Type::Primitive::i64)),
      value(value) {}

/**
 * @brief Dumps integer literal information
 *
 * Output format:
 *   [indent]IntLiteral: value
 *
 * @param level Current indentation level
 */
void IntLiteral::info_dump(int level) const {
    std::println("{}IntLiteral: {}", indent(level), value);
}

// ====================== FloatLiteral Implementation ========================//

/**
 * @brief Constructs a floating-point literal expression
 *
 * Default type is set to f64. The actual type may be
 * adjusted during semantic analysis based on value.
 *
 * @param location Source location of literal
 * @param value Floating-point value
 */
FloatLiteral::FloatLiteral(SrcLocation location, const double value)
    : Expr(std::move(location), Type(Type::Primitive::f64)),
      value(value) {}

/**
 * @brief Dumps float literal information
 *
 * Output format:
 *   [indent]FloatLiteral: value
 *
 * @param level Current indentation level
 */
void FloatLiteral::info_dump(int level) const {
    std::println("{}FloatLiteral: {}", indent(level), value);
}

// ====================== StrLiteral Implementation ========================//

/**
 * @brief Constructs a string literal expression
 *
 * Type is always set to str primitive type.
 *
 * @param location Source location of literal
 * @param value String content
 */
StrLiteral::StrLiteral(SrcLocation location, std::string value)
    : Expr(std::move(location), Type(Type::Primitive::str)),
      value(std::move(value)) {}

/**
 * @brief Dumps string literal information
 *
 * Output format:
 *   [indent]StrLiteral: value
 *
 * @param level Current indentation level
 */
void StrLiteral::info_dump(int level) const {
    std::println("{}StrLiteral: {}", indent(level), value);
}

// ====================== CharLiteral Implementation ========================//

/**
 * @brief Constructs a character literal expression
 *
 * Type is always set to char primitive type.
 *
 * @param location Source location of literal
 * @param value Character value
 */
CharLiteral::CharLiteral(SrcLocation location, char value)
    : Expr(std::move(location), Type(Type::Primitive::character)),
      value(value) {}

/**
 * @brief Dumps character literal information
 *
 * Output format:
 *   [indent]CharLiteral: value
 *
 * @param level Current indentation level
 */
void CharLiteral::info_dump(int level) const {
    std::println("{}CharLiteral: {}", indent(level), value);
}

// ====================== BoolLiteral Implementation ========================//

/**
 * @brief Constructs a boolean literal expression
 *
 * Type is always set to bool primitive type.
 *
 * @param location Source location of literal
 * @param value Boolean value
 */
BoolLiteral::BoolLiteral(SrcLocation location, bool value)
    : Expr(std::move(location), Type(Type::Primitive::boolean)),
      value(value) {}

/**
 * @brief Dumps boolean literal information
 *
 * Output format:
 *   [indent]BoolLiteral: value
 *
 * @param level Current indentation level
 */
void BoolLiteral::info_dump(int level) const {
    std::println("{}BoolLiteral: {}", indent(level), value);
}

// ====================== RangeLiteral Implementation ========================//

/**
 * @brief Constructs a range literal expression
 *
 * Represents inclusive (..=) or exclusive (..) ranges.
 * Type is set during semantic analysis.
 *
 * @param location Source location of range
 * @param start Start expression
 * @param end End expression
 * @param inclusive True for inclusive range, false for exclusive
 */
RangeLiteral::RangeLiteral(SrcLocation location,
                           std::unique_ptr<Expr> start,
                           std::unique_ptr<Expr> end,
                           const bool inclusive)
    : Expr(std::move(location)),
      start(std::move(start)),
      end(std::move(end)),
      inclusive(inclusive) {}

/**
 * @brief Dumps range literal information
 *
 * Output format:
 *   [indent]RangeLiteral:
 *     [indent]  start:
 *       [child expression dump]
 *     [indent]  end:
 *       [child expression dump]
 *
 * @param level Current indentation level
 */
void RangeLiteral::info_dump(int level) const {
    std::println("{}RangeLiteral:", indent(level));
    std::println("{}  start:", indent(level));
    start->info_dump(level + 2);
    std::println("{}  end:", indent(level));
    end->info_dump(level + 2);
}

// ====================== DeclRefExpr Implementation ========================//

/**
 * @brief Constructs a declaration reference expression
 *
 * Represents references to variables, functions, etc.
 * The actual declaration is resolved during semantic analysis.
 *
 * @param location Source location of reference
 * @param identifier Declaration name
 */
DeclRefExpr::DeclRefExpr(SrcLocation location, std::string identifier)
    : Expr(std::move(location)),
      identifier(std::move(identifier)) {}

/**
 * @brief Dumps declaration reference information
 *
 * Output format:
 *   [indent]DeclRefExpr: identifier
 *
 * @param level Current indentation level
 */
void DeclRefExpr::info_dump(int level) const {
    std::println("{}DeclRefExpr: {}", indent(level), identifier);
}

//====================== FunCallExpr Implementation ========================//

/**
 * @brief Constructs a function call expression
 *
 * @param location Source location of call
 * @param callee Expression being called (usually DeclRefExpr)
 * @param args Argument expressions
 */
FunCallExpr::FunCallExpr(SrcLocation location,
                         std::unique_ptr<Expr> callee,
                         std::vector<std::unique_ptr<Expr>> args)
    : Expr(std::move(location)),
      callee(std::move(callee)),
      args(std::move(args)) {}

/**
 * @brief Dumps function call information
 *
 * Output format:
 *   [indent]FunCallExpr
 *     [indent]  callee:
 *       [child expression dump]
 *     [indent]  args:
 *       [argument dumps]
 *
 * @param level Current indentation level
 */
void FunCallExpr::info_dump(int level) const {
    std::println("{}FunCallExpr", indent(level));
    std::println("{}  callee:", indent(level));
    callee->info_dump(level + 2);
    std::println("{}  args:", indent(level));
    for (const auto& arg : args) {
        arg->info_dump(level + 2);
    }
}

//====================== BinaryOp Implementation ========================//

/**
 * @brief Constructs a binary operation expression
 *
 * @param lhs Left-hand operand
 * @param rhs Right-hand operand
 * @param op Operator token
 */
BinaryOp::BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs, const Token& op)
    : Expr(op.get_start()),
      lhs(std::move(lhs)),
      rhs(std::move(rhs)),
      op(op.get_type()) {}

/**
 * @brief Dumps binary operation information
 *
 * Output format:
 *   [indent]BinaryOp: operator
 *     [indent]  lhs:
 *       [child expression dump]
 *     [indent]  rhs:
 *       [child expression dump]
 *   [indent]  type: resolved_type (if available)
 *
 * @param level Current indentation level
 */
void BinaryOp::info_dump(int level) const {
    std::println("{}BinaryOp: {}", indent(level), type_to_string(op));
    std::println("{}  lhs:", indent(level));
    lhs->info_dump(level + 2);
    std::println("{}  rhs:", indent(level));
    rhs->info_dump(level + 2);
    if (type.has_value()) {
        std::println("{}  type: {}", indent(level), type.value().to_string());
    }
}

//====================== UnaryOp Implementation ========================//

/**
 * @brief Constructs a unary operation expression
 *
 * @param operand Target expression
 * @param op Operator token
 * @param is_prefix True if prefix operator, false if postfix
 */
UnaryOp::UnaryOp(std::unique_ptr<Expr> operand, const Token& op, const bool is_prefix)
    : Expr(op.get_start()),
      operand(std::move(operand)),
      op(op.get_type()),
      is_prefix(is_prefix) {}

/**
 * @brief Dumps unary operation information
 *
 * Output format:
 *   [indent]UnaryOp: operator
 *     [indent]  expr:
 *       [child expression dump]
 *
 * @param level Current indentation level
 */
void UnaryOp::info_dump(int level) const {
    std::println("{}UnaryOp: {}", indent(level), type_to_string(op));
    std::println("{}  expr:", indent(level));
    operand->info_dump(level + 2);
}

} // namespace phi
