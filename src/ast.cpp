#include "ast.hpp"
#include <print>
#include <string>

void Block::info_dump(int level) const { std::println("{}Block", std::string(level * 2, ' ')); }

void FunctionDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 std::string(level * 2, ' '), // indent
                 identifier,
                 location.line,
                 location.col,
                 return_type.to_string());
    block->info_dump(level + 1);
}

void ReturnStmt::info_dump(int level) const {
    std::println("{}ReturnStmt", std::string(level * 2, ' '));
    if (expr) {
        expr->info_dump(level + 1);
    }
}

void IntLiteral::info_dump(int level) const {
    std::println("{}IntLiteral: {}", std::string(level * 2, ' '), value);
}

void FloatLiteral::info_dump(int level) const {
    std::println("{}FloatLiteral: {}", std::string(level * 2, ' '), value);
}

void DeclRefExpr::info_dump(int level) const {
    std::println("{}DeclRefExpr: {}", std::string(level * 2, ' '), identifier);
}
