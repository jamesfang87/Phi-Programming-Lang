#include "AST/Expr.hpp"
#include "AST/Decl.hpp"

#include <print>

#include "Lexer/TokenType.hpp"

void IntLiteral::info_dump(const int level) const {
    std::println("{}IntLiteral: {}", std::string(level * 2, ' '), value);
}

void FloatLiteral::info_dump(const int level) const {
    std::println("{}FloatLiteral: {}", std::string(level * 2, ' '), value);
}

void StrLiteral::info_dump(const int level) const {
    std::println("{}StrLiteral: {}", std::string(level * 2, ' '), value);
}

void CharLiteral::info_dump(const int level) const {
    std::println("{}CharLiteral: {}", std::string(level * 2, ' '), value);
}

void BoolLiteral::info_dump(const int level) const {
    std::println("{}BoolLiteral: {}", std::string(level * 2, ' '), value);
}

void RangeLiteral::info_dump(const int level) const {
    std::println("{}RangeLiteral:", std::string(level * 2, ' '));
    std::println("{}  start:", std::string(level * 2, ' '));
    start->info_dump(level + 2);
    std::println("{}  end:", std::string(level * 2, ' '));
    end->info_dump(level + 2);
}

void DeclRefExpr::info_dump(const int level) const {
    if (decl == nullptr)
        std::println("{}DeclRefExpr: {}", std::string(level * 2, ' '), identifier);
    else {
        std::print("{}DeclRefExpr: {} referencing ", std::string(level * 2, ' '), identifier);
        decl->info_dump(0);
    }
}

void FunCallExpr::info_dump(const int level) const {
    std::println("{}FunCallExpr", std::string(level * 2, ' '));
    std::println("{}  callee:", std::string(level * 2, ' '));
    if (func_decl != nullptr) {
        func_decl->info_dump(0);
    }
    callee->info_dump(level + 2);
    std::println("{}  args:", std::string(level * 2, ' '));
    for (const auto& arg : args) {
        arg->info_dump(level + 2);
    }
}

void BinaryOp::info_dump(const int level) const {
    std::println("{}BinaryOp: {}", std::string(level * 2, ' '), type_to_string(op));
    std::println("{}  lhs:", std::string(level * 2, ' '));
    lhs->info_dump(level + 2);
    std::println("{}  rhs:", std::string(level * 2, ' '));
    rhs->info_dump(level + 2);
    if (type.has_value()) {
        std::println("{}  type: {}", std::string(level * 2, ' '), type.value().to_string());
    }
}

void UnaryOp::info_dump(const int level) const {
    std::println("{}UnaryOp: {}", std::string(level * 2, ' '), type_to_string(op));
    std::println("{}  expr:", std::string(level * 2, ' '));
    operand->info_dump(level + 2);
}
