#include "AST/Expr.hpp"
#include <print>

void IntLiteral::info_dump(int level) const {
    std::println("{}IntLiteral: {}", std::string(level * 2, ' '), value);
}

void FloatLiteral::info_dump(int level) const {
    std::println("{}FloatLiteral: {}", std::string(level * 2, ' '), value);
}

void StrLiteral::info_dump(int level) const {
    std::println("{}StrLiteral: {}", std::string(level * 2, ' '), value);
}

void CharLiteral::info_dump(int level) const {
    std::println("{}CharLiteral: {}", std::string(level * 2, ' '), value);
}

void DeclRefExpr::info_dump(int level) const {
    std::println("{}DeclRefExpr: {}", std::string(level * 2, ' '), identifier);
}

void FunctionCall::info_dump(int level) const {
    std::println("{}FunCallExpr", std::string(level * 2, ' '));
    std::println("{}  callee:", std::string(level * 2, ' '));
    callee->info_dump(level + 2);
    std::println("{}  args:", std::string(level * 2, ' '));
    for (const auto& arg : args) {
        arg->info_dump(level + 2);
    }
}
