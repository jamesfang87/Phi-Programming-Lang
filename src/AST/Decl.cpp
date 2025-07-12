#include "AST/Decl.hpp"
#include <print>

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

void Block::info_dump(int level) const {
    std::println("{}Block", std::string(level * 2, ' '));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}
