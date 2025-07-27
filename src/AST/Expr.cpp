#include "AST/Expr.hpp"

#include <cassert>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "Lexer/TokenType.hpp"

namespace {
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

//======================== IntLiteral ========================//
IntLiteral::IntLiteral(SrcLocation location, const int64_t value)
    : Expr(std::move(location), Type(Type::Primitive::i64)),
      value(value) {}

void IntLiteral::info_dump(int level) const {
    std::println("{}IntLiteral: {}", indent(level), value);
}

// ====================== FloatLiteral ========================//
FloatLiteral::FloatLiteral(SrcLocation location, const double value)
    : Expr(std::move(location), Type(Type::Primitive::f64)),
      value(value) {}

void FloatLiteral::info_dump(int level) const {
    std::println("{}FloatLiteral: {}", indent(level), value);
}

// ====================== StrLiteral ========================//
StrLiteral::StrLiteral(SrcLocation location, std::string value)
    : Expr(std::move(location), Type(Type::Primitive::str)),
      value(std::move(value)) {}

void StrLiteral::info_dump(int level) const {
    std::println("{}StrLiteral: {}", indent(level), value);
}

// ====================== CharLiteral ========================//
CharLiteral::CharLiteral(SrcLocation location, char value)
    : Expr(std::move(location), Type(Type::Primitive::character)),
      value(value) {}

void CharLiteral::info_dump(int level) const {
    std::println("{}CharLiteral: {}", indent(level), value);
}

// ====================== BoolLiteral ========================//
BoolLiteral::BoolLiteral(SrcLocation location, bool value)
    : Expr(std::move(location), Type(Type::Primitive::boolean)),
      value(value) {}

void BoolLiteral::info_dump(int level) const {
    std::println("{}BoolLiteral: {}", indent(level), value);
}

// ====================== RangeLiteral ========================//
RangeLiteral::RangeLiteral(SrcLocation location,
                           std::unique_ptr<Expr> start,
                           std::unique_ptr<Expr> end,
                           const bool inclusive)
    : Expr(std::move(location)),
      start(std::move(start)),
      end(std::move(end)),
      inclusive(inclusive) {}

void RangeLiteral::info_dump(int level) const {
    std::println("{}RangeLiteral:", indent(level));
    std::println("{}  start:", indent(level));
    start->info_dump(level + 2);
    std::println("{}  end:", indent(level));
    end->info_dump(level + 2);
}

// ====================== DeclRefExpr ========================//
DeclRefExpr::DeclRefExpr(SrcLocation location, std::string identifier)
    : Expr(std::move(location)),
      identifier(std::move(identifier)) {}

void DeclRefExpr::info_dump(int level) const {
    if (decl == nullptr)
        std::println("{}DeclRefExpr: {}", indent(level), identifier);
    else {
        std::print("{}DeclRefExpr: {} referencing ", indent(level), identifier);
        decl->info_dump(0);
    }
}

//====================== FunCallExpr ========================//
FunCallExpr::FunCallExpr(SrcLocation location,
                         std::unique_ptr<Expr> callee,
                         std::vector<std::unique_ptr<Expr>> args)
    : Expr(std::move(location)),
      callee(std::move(callee)),
      args(std::move(args)) {}

void FunCallExpr::info_dump(int level) const {
    std::println("{}FunCallExpr", indent(level));
    std::println("{}  callee:", indent(level));
    if (func_decl != nullptr) {
        func_decl->info_dump(0);
    }
    callee->info_dump(level + 2);
    std::println("{}  args:", indent(level));
    for (const auto& arg : args) {
        arg->info_dump(level + 2);
    }
}

//====================== BinaryOp ========================//
BinaryOp::BinaryOp(std::unique_ptr<Expr> lhs, std::unique_ptr<Expr> rhs, const Token& op)
    : Expr(op.get_start()),
      lhs(std::move(lhs)),
      rhs(std::move(rhs)),
      op(op.get_type()) {}

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

//====================== UnaryOp ========================//
UnaryOp::UnaryOp(std::unique_ptr<Expr> operand, const Token& op, const bool is_prefix)
    : Expr(op.get_start()),
      operand(std::move(operand)),
      op(op.get_type()),
      is_prefix(is_prefix) {}

void UnaryOp::info_dump(int level) const {
    std::println("{}UnaryOp: {}", indent(level), type_to_string(op));
    std::println("{}  expr:", indent(level));
    operand->info_dump(level + 2);
}
