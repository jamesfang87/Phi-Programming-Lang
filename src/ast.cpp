#include "ast.hpp"
#include <print>
#include <string>

void Block::info_dump(int level) const {
    std::println("{}Block", std::string(level * 2, ' '));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}

void ParamDecl::info_dump(int level) const {
    std::println("{}ParamDecl: {} (type: {})",
                 std::string(level * 2, ' '),
                 identifier,
                 type.to_string());
}

void FunctionDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 std::string(level * 2, ' '), // indent
                 identifier,
                 location.line,
                 location.col,
                 return_type.to_string());
    for (auto& p : *params) {
        p->info_dump(level + 1);
    }
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

void StrLiteral::info_dump(int level) const {
    std::println("{}StrLiteral: {}", std::string(level * 2, ' '), value);
}

void CharLiteral::info_dump(int level) const {
    std::println("{}CharLiteral: {}", std::string(level * 2, ' '), value);
}

void DeclRef::info_dump(int level) const {
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

// Resolved AST implementations
void ResolvedBlock::info_dump(size_t level) const {
    std::println("{}ResolvedBlock", std::string(level * 2, ' '));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}

void ResolvedParamDecl::info_dump(size_t level) const {
    std::println("{}ResolvedParamDecl: {} (type: {})",
                 std::string(level * 2, ' '),
                 identifier,
                 type.to_string());
}

void ResolvedFunDecl::info_dump(size_t level) const {
    std::println("{}ResolvedFunction {} at {}:{}. Returns {}",
                 std::string(level * 2, ' '), // indent
                 identifier,
                 location.line,
                 location.col,
                 type.to_string());
    for (auto& p : params) {
        p->info_dump(level + 1);
    }
    if (body) {
        body->info_dump(level + 1);
    }
}

void ResolvedExpr::info_dump(size_t level) const {
    std::println("{}ResolvedExpr (type: {})", std::string(level * 2, ' '), type.to_string());
}

void ResolvedIntLiteral::info_dump(size_t level) const {
    std::println("{}ResolvedIntLiteral: {}", std::string(level * 2, ' '), value);
}

void ResolvedFloatLiteral::info_dump(size_t level) const {
    std::println("{}ResolvedFloatLiteral: {}", std::string(level * 2, ' '), value);
}

void ResolvedDeclRef::info_dump(size_t level) const {
    std::println("{}ResolvedDeclRefExpr: {}", std::string(level * 2, ' '), identifier);
}

void ResolvedFunctionCall::info_dump(size_t level) const {
    std::println("{}ResolvedFunCallExpr", std::string(level * 2, ' '));
    std::println("{}  callee:", std::string(level * 2, ' '));
    callee->info_dump(level + 2);
    std::println("{}  args:", std::string(level * 2, ' '));
    for (const auto& arg : args) {
        arg->info_dump(level + 2);
    }
}

void ResolvedReturnStmt::info_dump(size_t level) const {
    std::println("{}ResolvedReturnStmt", std::string(level * 2, ' '));
    if (expr) {
        expr->info_dump(level + 1);
    }
}
