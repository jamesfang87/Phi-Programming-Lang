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

namespace phi {

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
VarDecl::VarDecl(SrcLocation location, std::string identifier, Type type,
                 const bool is_const, std::unique_ptr<Expr> initializer)
    : Decl(std::move(location), std::move(identifier), std::move(type)),
      IsConst(is_const), init(std::move(initializer)) {}

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
void VarDecl::emit(int level) const {
  std::println("{}VarDecl: {} (type: {})", indent(level), id,
               type.value().toString());
  if (init) {
    std::println("{}Initializer:", indent(level));
    init->emit(level + 1);
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
ParamDecl::ParamDecl(SrcLocation location, std::string identifier, Type type,
                     bool IsConst)
    : Decl(std::move(location), std::move(identifier), std::move(type)),
      IsConst(IsConst) {}

/**
 * @brief Dumps parameter declaration information
 *
 * Output format:
 *   [indent]ParamDecl: name (type: type)
 *
 * @param level Current indentation level
 */
void ParamDecl::emit(int level) const {
  std::println("{}ParamDecl: {} (type: {})", indent(level), id,
               type.value().toString());
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
FunDecl::FunDecl(SrcLocation location, std::string identifier, Type return_type,
                 std::vector<std::unique_ptr<ParamDecl>> params,
                 std::unique_ptr<Block> block_ptr)
    : Decl(std::move(location), std::move(identifier), std::move(return_type)),
      params(std::move(params)), block(std::move(block_ptr)) {}

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
void FunDecl::emit(int level) const {
  std::println("{}Function {} at {}:{}. Returns {}", indent(level), id,
               location.line, location.col, type.value().toString());
  // Dump parameters
  for (auto &p : params) {
    p->emit(level + 1);
  }
  // Dump function body
  block->emit(level + 1);
}

} // namespace phi
