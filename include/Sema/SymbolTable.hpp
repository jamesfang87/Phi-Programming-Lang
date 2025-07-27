#pragma once

#include <cstddef>
#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"

namespace phi {

class SymbolTable {
public:
    class ScopeGuard {
        SymbolTable& table;

    public:
        explicit ScopeGuard(SymbolTable& table)
            : table(table) {
            table.enter_scope();
        }
        ~ScopeGuard() { table.exit_scope(); }

        // Non-copyable, non-movable
        ScopeGuard(const ScopeGuard&) = delete;
        ScopeGuard& operator=(const ScopeGuard&) = delete;
        ScopeGuard(ScopeGuard&&) = delete;
        ScopeGuard& operator=(ScopeGuard&&) = delete;
    };

    bool insert_decl(Decl* decl);
    Decl* lookup_decl(const std::string& name);

private:
    std::vector<std::unordered_map<std::string, Decl*>> scopes;

    void enter_scope();
    void exit_scope();
};

} // namespace phi
