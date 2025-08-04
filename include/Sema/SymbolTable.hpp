#pragma once

#include <cstddef>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace phi {

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
  /**
   * @brief RAII scope management helper for automatic scope handling
   *
   * This class provides exception-safe scope management by automatically
   * entering a new scope when constructed and exiting the scope when
   * destroyed (typically when going out of scope). This ensures proper
   * resource management and prevents scope leaks.
   */
  class ScopeGuard {
    SymbolTable &table; ///< Reference to the parent symbol table

  public:
    /**
     * @brief Constructs a ScopeGuard and enters a new scope
     * @param table Reference to the symbol table to manage
     */
    explicit ScopeGuard(SymbolTable &table) : table(table) {
      table.enter_scope();
    }

    /**
     * @brief Destructor that automatically exits the scope
     */
    ~ScopeGuard() { table.exit_scope(); }

    // Explicitly disable copying and moving to prevent accidental scope
    // mismanagement
    ScopeGuard(const ScopeGuard &) = delete;
    ScopeGuard &operator=(const ScopeGuard &) = delete;
    ScopeGuard(ScopeGuard &&) = delete;
    ScopeGuard &operator=(ScopeGuard &&) = delete;
  };

  struct Scope {
    std::unordered_map<std::string, Decl *> vars;
    std::unordered_map<std::string, FunDecl *> funs;
    // std::vector<std::unordered_map<std::string, Decl*>> structs;
  };

  bool insert(FunDecl *fun);
  bool insert(Decl *s);
  bool insert(VarDecl *var);
  bool insert(ParamDecl *param);

  FunDecl *lookup(FunCallExpr &call);
  Decl *lookup_struct(const std::string &name);
  Decl *lookup(DeclRefExpr &var);

private:
  /// Stack of scopes, with the back being the innermost current scope
  std::vector<Scope> scopes;

  /**
   * @brief Enters a new scope
   *
   * Pushes a new empty scope onto the scope stack. This should be called
   * when entering any block that introduces a new lexical scope (function
   * bodies, control structures, etc.).
   */
  void enter_scope();

  /**
   * @brief Exits the current innermost scope
   *
   * Pops the current scope from the scope stack, effectively ending the
   * current lexical scope. All declarations in this scope become inaccessible.
   */
  void exit_scope();
};

} // namespace phi
