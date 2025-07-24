#include "AST/Decl.hpp"

#include <print>
#include <string>

#include "AST/Expr.hpp"

namespace {
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

void ParamDecl::info_dump(int level) const {
    std::println("{}ParamDecl: {} (type: {})", indent(level), get_id(), type.value().to_string());
}

void VarDecl::info_dump(int level) const {
    std::println("{}VarDecl: {} (type: {})", indent(level), get_id(), type.value().to_string());
    if (initializer) {
        std::println("{}Initializer:", indent(level));
        initializer->info_dump(level + 1);
    }
}

void FunDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 indent(level),
                 get_id(),
                 location.line,
                 location.col,
                 type.value().to_string());
    for (auto& p : params) {
        p->info_dump(level + 1);
    }
    block->info_dump(level + 1);
}

void Block::info_dump(int level) const {
    std::println("{}Block", indent(level));
    for (auto& s : this->stmts) {
        s->info_dump(level + 1);
    }
}
