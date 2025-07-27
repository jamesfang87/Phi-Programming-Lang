#pragma once

#include <cstddef>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"

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
        SymbolTable& table; ///< Reference to the parent symbol table

    public:
        /**
         * @brief Constructs a ScopeGuard and enters a new scope
         * @param table Reference to the symbol table to manage
         */
        explicit ScopeGuard(SymbolTable& table)
            : table(table) {
            table.enter_scope();
        }

        /**
         * @brief Destructor that automatically exits the scope
         */
        ~ScopeGuard() { table.exit_scope(); }

        // Explicitly disable copying and moving to prevent accidental scope mismanagement
        ScopeGuard(const ScopeGuard&) = delete;
        ScopeGuard& operator=(const ScopeGuard&) = delete;
        ScopeGuard(ScopeGuard&&) = delete;
        ScopeGuard& operator=(ScopeGuard&&) = delete;
    };

    /**
     * @brief Inserts a declaration into the current innermost scope
     *
     * @param decl Pointer to the declaration to insert
     * @return true if the declaration was successfully inserted
     * @return false if an identifier with the same name already exists
     *         in the current scope (name collision)
     *
     * @note This method only checks the current scope for collisions,
     *       allowing shadowing of identifiers in outer scopes.
     */
    bool insert_decl(Decl* decl);

    /**
     * @brief Looks up a declaration by name using lexical scoping rules
     *
     * Performs identifier resolution by searching from the innermost scope
     * outward until a matching declaration is found or all scopes are exhausted.
     *
     * @param name The identifier to search for
     * @return Decl* Pointer to the found declaration, or nullptr if no
     *         declaration with the given name exists in any accessible scope
     */
    Decl* lookup_decl(const std::string& name);

private:
    /// Stack of scopes, with the back being the innermost current scope
    std::vector<std::unordered_map<std::string, Decl*>> scopes;

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
