#include "Sema/SymbolTable.hpp"

namespace phi {

/**
 * Enters a new scope in the symbol table.
 *
 * Scopes are implemented as stack of hash maps.
 * Each scope corresponds to a lexical block (function, if, for, etc.).
 */
void SymbolTable::enter_scope() { scopes.emplace_back(); }

/**
 * Exits the current scope, discarding all declarations within it.
 *
 * Automatically removes all symbols defined in the current scope.
 */
void SymbolTable::exit_scope() { scopes.pop_back(); }

/**
 * Inserts a declaration into the current scope.
 *
 * @param decl Declaration to insert
 * @return true if insertion succeeded, false if identifier already exists
 *
 * Prevents redeclaration in the same scope while allowing shadowing in inner scopes.
 */
bool SymbolTable::insert_decl(Decl* decl) {
    if (lookup_decl(decl->get_id())) {
        return false; // Already exists in current scope
    }
    scopes.back()[decl->get_id()] = decl;
    return true;
}

/**
 * Looks up a declaration by name using lexical scoping rules.
 *
 * @param name Identifier to search for
 * @return Pointer to declaration if found, nullptr otherwise
 *
 * Searches from innermost to outermost scope (LIFO order).
 */
Decl* SymbolTable::lookup_decl(const std::string& name) {
    for (int i = scopes.size() - 1; i >= 0; --i) {
        if (auto it = scopes[i].find(name); it != scopes[i].end()) {
            return it->second;
        }
    }
    return nullptr;
}

} // namespace phi
