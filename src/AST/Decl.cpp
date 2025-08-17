#include "AST/Decl.hpp"

#include <print>
#include <string>

#include "AST/Expr.hpp"

namespace {
/// Generates indentation string for AST dumping
/// @param Level Current indentation Level
/// @return String of spaces for indentation
std::string indent(int Level) { return std::string(Level * 2, ' '); }
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
 * @param Level Current indentation Level
 */
void VarDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::println("{}VarDecl: {} (type: {})", indent(Level), Id, typeStr);
  if (Init) {
    std::println("{}Initializer:", indent(Level));
    Init->emit(Level + 1);
  }
}

//======================== ParamDecl Implementation ========================//

/**
 * @brief Dumps parameter declaration information
 *
 * Output format:
 *   [indent]ParamDecl: name (type: type)
 *
 * @param Level Current indentation Level
 */
void ParamDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::println("{}ParamDecl: {} (type: {})", indent(Level), Id, typeStr);
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
 * @param Level Current indentation Level
 */
void FunDecl::emit(int Level) const {
  std::println("{}Function {} at {}:{}. Returns {}", indent(Level), Id,
               Location.line, Location.col, ReturnType.toString());
  // Dump parameters
  for (auto &p : Params) {
    p->emit(Level + 1);
  }
  // Dump function body
  Body->emit(Level + 1);
}

void StructDecl::emit(int Level) const {
  std::println("{}StructDecl: {} (type: {})", indent(Level), Id,
               Ty.getCustomTypeName());

  std::println("{}Fields:", indent(Level));
  for (auto &f : Fields) {
    f.emit(Level + 1);
  }

  std::println("{}Methods:", indent(Level));
  for (auto &m : Methods) {
    m.emit(Level + 1);
  }
}

void FieldDecl::emit(int Level) const {
  std::string typeStr =
      DeclType.has_value() ? DeclType.value().toString() : "<unresolved>";
  std::string visibility = isPrivate() ? "private" : "public";
  std::println("{}{} FieldDecl: {} (type: {})", indent(Level), visibility, Id,
               typeStr);
}

} // namespace phi
