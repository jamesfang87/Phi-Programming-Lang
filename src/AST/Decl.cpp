#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

#include <print>

void ParamDecl::info_dump(const int level) const {
    std::println("{}ParamDecl: {} (type: {})",
                 std::string(level * 2, ' '),
                 get_id(),
                 type.value().to_string());
}

void VarDecl::info_dump(const int level) const {
    std::println("{}VarDecl: {} (type: {})",
                 std::string(level * 2, ' '),
                 get_id(),
                 type.value().to_string());
    if (initializer) {
        std::println("{}Initializer:", std::string(level * 2, ' '));
        initializer->info_dump(level + 1);
    }
}

void FunDecl::info_dump(const int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 std::string(level * 2, ' '), // indent
                 get_id(),
                 location.line,
                 location.col,
                 type.value().to_string());
    for (auto& p : params) {
        p->info_dump(level + 1);
    }
    block->info_dump(level + 1);
}

void Block::info_dump(const int level) const {
    std::println("{}Block", std::string(level * 2, ' '));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}
