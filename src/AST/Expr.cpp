#include "AST/Expr.hpp"

#include <cassert>
#include <print>
#include <string>

#include "AST/Decl.hpp"
#include "Lexer/TokenType.hpp"

namespace {
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

void IntLiteral::info_dump(int level) const {
    std::println("{}IntLiteral: {}", indent(level), value);
}

void FloatLiteral::info_dump(int level) const {
    std::println("{}FloatLiteral: {}", indent(level), value);
}

void StrLiteral::info_dump(int level) const {
    std::println("{}StrLiteral: {}", indent(level), value);
}

void CharLiteral::info_dump(int level) const {
    std::println("{}CharLiteral: {}", indent(level), value);
}

void BoolLiteral::info_dump(int level) const {
    std::println("{}BoolLiteral: {}", indent(level), value);
}

void RangeLiteral::info_dump(int level) const {
    std::println("{}RangeLiteral:", indent(level));
    std::println("{}  start:", indent(level));
    start->info_dump(level + 2);
    std::println("{}  end:", indent(level));
    end->info_dump(level + 2);
}

void DeclRefExpr::info_dump(int level) const {
    if (decl == nullptr)
        std::println("{}DeclRefExpr: {}", indent(level), identifier);
    else {
        std::print("{}DeclRefExpr: {} referencing ", indent(level), identifier);
        decl->info_dump(0);
    }
}

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

void UnaryOp::info_dump(int level) const {
    std::println("{}UnaryOp: {}", indent(level), type_to_string(op));
    std::println("{}  expr:", indent(level));
    operand->info_dump(level + 2);
}
