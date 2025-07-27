#include "Sema/SymbolTable.hpp"

void SymbolTable::enter_scope() { scopes.emplace_back(); }
void SymbolTable::exit_scope() { scopes.pop_back(); }

bool SymbolTable::insert_decl(Decl* decl) {
    if (lookup_decl(decl->get_id())) {
        return false; // Already exists in current scope
    }
    scopes.back()[decl->get_id()] = decl;
    return true;
}

Decl* SymbolTable::lookup_decl(const std::string& name) {
    for (int i = scopes.size() - 1; i >= 0; --i) {
        if (auto it = scopes[i].find(name); it != scopes[i].end()) {
            return it->second;
        }
    }
    return nullptr;
}
