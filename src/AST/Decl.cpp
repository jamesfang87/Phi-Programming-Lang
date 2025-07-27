#include "AST/Decl.hpp"

#include <print>
#include <string>

#include "AST/Expr.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param level Current indentation level
/// @return String of spaces for indentation
std::string indent(int level) { return std::string(level * 2, ' '); }
} // namespace

//======================== VarDecl Implementation ========================//

/**
 * @brief Constructs a variable declaration node
 *
 * @param location Source location of declaration
 * @param identifier Variable name
 * @param type Variable type
 * @param is_const Constant declaration flag
 * @param initializer Initial value expression
 */
VarDecl::VarDecl(SrcLocation location,
                 std::string identifier,
                 Type type,
                 const bool is_const,
                 std::unique_ptr<Expr> initializer)
    : Decl(std::move(location), std::move(identifier), std::move(type)),
      is_const(is_const),
      initializer(std::move(initializer)) {}

/**
 * @brief Dumps variable declaration information for debugging
 *
 * Output format:
 *   [indent]VarDecl: name (type: type)
 *   [indent]Initializer:
 *     [child expression dump]
 *
 * @param level Current indentation level
 */
void VarDecl::info_dump(int level) const {
    std::println("{}VarDecl: {} (type: {})", indent(level), identifier, type.value().to_string());
    if (initializer) {
        std::println("{}Initializer:", indent(level));
        initializer->info_dump(level + 1);
    }
}

//======================== ParamDecl Implementation ========================//

/**
 * @brief Constructs a function parameter declaration
 *
 * @param location Source location of parameter
 * @param identifier Parameter name
 * @param type Parameter type
 */
ParamDecl::ParamDecl(SrcLocation location, std::string identifier, Type type)
    : Decl(std::move(location), std::move(identifier), std::move(type)) {}

/**
 * @brief Dumps parameter declaration information
 *
 * Output format:
 *   [indent]ParamDecl: name (type: type)
 *
 * @param level Current indentation level
 */
void ParamDecl::info_dump(int level) const {
    std::println("{}ParamDecl: {} (type: {})", indent(level), identifier, type.value().to_string());
}

//======================== FunDecl Implementation ========================//

/**
 * @brief Constructs a function declaration
 *
 * @param location Source location of function
 * @param identifier Function name
 * @param return_type Function return type
 * @param params Function parameters
 * @param block_ptr Function body block
 */
FunDecl::FunDecl(SrcLocation location,
                 std::string identifier,
                 Type return_type,
                 std::vector<std::unique_ptr<ParamDecl>> params,
                 std::unique_ptr<Block> block_ptr)
    : Decl(std::move(location), std::move(identifier), std::move(return_type)),
      params(std::move(params)),
      block(std::move(block_ptr)) {}

/**
 * @brief Dumps function declaration information
 *
 * Output format:
 *   [indent]Function name at line:col. Returns type
 *   [param dumps]
 *   [block dump]
 *
 * @param level Current indentation level
 */
void FunDecl::info_dump(int level) const {
    std::println("{}Function {} at {}:{}. Returns {}",
                 indent(level),
                 identifier,
                 location.line,
                 location.col,
                 type.value().to_string());
    // Dump parameters
    for (auto& p : params) {
        p->info_dump(level + 1);
    }
    // Dump function body
    block->info_dump(level + 1);
}
