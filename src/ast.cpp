#include "ast.hpp"
#include <print>
#include <string>

void Block::info_dump(int level) const { std::println("{}Block", std::string(level * 2, ' ')); }

void FunctionDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 std::string(level * 2, ' '), // indent
                 identifier,
                 line,
                 col,
                 return_type.to_string());
    block->info_dump(level + 1);
}
