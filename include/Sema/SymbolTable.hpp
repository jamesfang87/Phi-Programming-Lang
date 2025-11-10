#pragma once

#include <cstddef>
#include <string>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// SymbolTable - Symbol table implementation for semantic analysis
//===----------------------------------------------------------------------===//

/**
 * @brief Symbol table implementation for semantic analysis
 *
 * Manages nested scopes and declaration lookups during compilation. The symbol
 * table is implemented as a stack of scopes, where each scope is a mapping
 * from identifiers to their corresponding declarations. Provides RAII-based
 * scope management through the ScopeGuard helper class to ensure proper
 * scope entry and exit even in the presence of exceptions.
 */
class SymbolTable {
public:
  //===--------------------------------------------------------------------===//
  // ScopeGuard - RAII scope management helper
  //===--------------------------------------------------------------------===//

  /**
   * @brief RAII scope management helper for automatic scope handling
   *
   * This class provides exception-safe scope management by automatically
   * entering a new scope when constructed and exiting the scope when
   * destroyed (typically when going out of scope). This ensures proper
   * resource management and prevents scope leaks.
   */
  class ScopeGuard {
  public:
    /**
     * @brief Constructs a ScopeGuard and enters a new scope
     * @param table Reference to the symbol table to manage
     */
    explicit ScopeGuard(SymbolTable &Table) : SymbolTab(Table) {
      Table.enterScope();
    }

    ~ScopeGuard() { SymbolTab.exitScope(); }
    ScopeGuard(const ScopeGuard &) = delete;
    ScopeGuard &operator=(const ScopeGuard &) = delete;
    ScopeGuard(ScopeGuard &&) = delete;
    ScopeGuard &operator=(ScopeGuard &&) = delete;

  private:
    SymbolTable &SymbolTab; ///< Reference to the parent symbol table
  };

  //===--------------------------------------------------------------------===//
  // Scope Structure Definition
  //===--------------------------------------------------------------------===//

  struct Scope {
    std::unordered_map<std::string, ValueDecl *> Vars;
    std::unordered_map<std::string, FunDecl *> Funs;
    std::unordered_map<std::string, StructDecl *> Structs;
    std::unordered_map<std::string, EnumDecl *> Enums;
  };

  //===--------------------------------------------------------------------===//
  // Declaration Insertion Methods
  //===--------------------------------------------------------------------===//

  bool insert(FunDecl *Fun);
  bool insert(StructDecl *Struct);
  bool insert(EnumDecl *Struct);
  bool insert(VarDecl *Var);
  bool insert(ParamDecl *Param);
  bool insert(FieldDecl *Field);

  //===--------------------------------------------------------------------===//
  // Symbol Lookup Methods
  //===--------------------------------------------------------------------===//

  FunDecl *lookup(FunCallExpr &Fun);
  ValueDecl *lookup(DeclRefExpr &Var);
  StructDecl *lookupStruct(const std::string &Id);
  EnumDecl *lookupEnum(const std::string &Id);

  FunDecl *lookup(FunDecl &Fun);
  StructDecl *lookup(StructDecl &Struct);
  EnumDecl *lookup(EnumDecl &Struct);
  VarDecl *lookup(VarDecl &Var);
  ParamDecl *lookup(ParamDecl &Param);
  FieldDecl *lookup(FieldDecl &Field);

  //===--------------------------------------------------------------------===//
  // Error Recovery & Suggestion Methods
  //===--------------------------------------------------------------------===//

  [[nodiscard]] FunDecl *getClosestFun(const std::string &Undeclared) const;
  [[nodiscard]] Decl *getClosestCustom(const std::string &Undeclared) const;
  [[nodiscard]] EnumDecl *getClosestEnum(const std::string &Undeclared) const;
  [[nodiscard]] ValueDecl *getClosestVar(const std::string &Undeclared) const;
  [[nodiscard]] std::optional<std::string>
  getClosestType(const std::string &Undeclared) const;

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  /// Stack of scopes, with the back being the innermost current scope
  std::vector<Scope> Scopes;

  //===--------------------------------------------------------------------===//
  // Scope Management Methods
  //===--------------------------------------------------------------------===//

  /**
   * @brief Enters a new scope
   *
   * Pushes a new empty scope onto the scope stack. This should be called
   * when entering any block that introduces a new lexical scope (function
   * bodies, control structures, etc.).
   */
  void enterScope();

  /**
   * @brief Exits the current innermost scope
   *
   * Pops the current scope from the scope stack, effectively ending the
   * current lexical scope. All declarations in this scope become
   * inaccessible.
   */
  void exitScope();
};

} // namespace phi
