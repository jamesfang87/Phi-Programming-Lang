#include "AST/Decl.hpp"

#include <print>
#include <string>

#include "AST/Expr.hpp"

namespace {
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

//======================== VarDecl ========================//
VarDecl::VarDecl(SrcLocation location,
                 std::string identifier,
                 Type type,
                 const bool is_const,
                 std::unique_ptr<Expr> initializer)
    : Decl(std::move(location), std::move(identifier), std::move(type)),
      is_const(is_const),
      initializer(std::move(initializer)) {}

void VarDecl::info_dump(int level) const {
    std::println("{}VarDecl: {} (type: {})", indent(level), identifier, type.value().to_string());
    if (initializer) {
        std::println("{}Initializer:", indent(level));
        initializer->info_dump(level + 1);
    }
}

//======================== ParamDecl ========================//
ParamDecl::ParamDecl(SrcLocation location, std::string identifier, Type type)
    : Decl(std::move(location), std::move(identifier), std::move(type)) {}

void ParamDecl::info_dump(int level) const {
    std::println("{}ParamDecl: {} (type: {})", indent(level), identifier, type.value().to_string());
}

//======================== FunDecl ========================//
FunDecl::FunDecl(SrcLocation location,
                 std::string identifier,
                 Type return_type,
                 std::vector<std::unique_ptr<ParamDecl>> params,
                 std::unique_ptr<Block> block_ptr)
    : Decl(std::move(location), std::move(identifier), std::move(return_type)),
      params(std::move(params)),
      block(std::move(block_ptr)) {}

void FunDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 indent(level),
                 identifier,
                 location.line,
                 location.col,
                 type.value().to_string());
    for (auto& p : params) {
        p->info_dump(level + 1);
    }
    block->info_dump(level + 1);
}
