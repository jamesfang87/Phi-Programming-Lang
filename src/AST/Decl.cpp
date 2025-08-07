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
  std::println("{}VarDecl: {} (type: {})", indent(level), Id,
               DeclType.value().toString());
  if (Init) {
    std::println("{}Initializer:", indent(level));
    Init->emit(level + 1);
  }
}

//======================== ParamDecl Implementation ========================//

/**
 * @brief Dumps parameter declaration information
 *
 * Output format:
 *   [indent]ParamDecl: name (type: type)
 *
 * @param level Current indentation level
 */
void ParamDecl::emit(int level) const {
  std::println("{}ParamDecl: {} (type: {})", indent(level), Id,
               DeclType.value().toString());
}

//======================== FunDecl Implementation ========================//

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
  std::println("{}Function {} at {}:{}. Returns {}", indent(level), Id,
               Location.line, Location.col, DeclType.value().toString());
  // Dump parameters
  for (auto &p : Params) {
    p->emit(level + 1);
  }
  // Dump function body
  Block->emit(level + 1);
}

} // namespace phi
