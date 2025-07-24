#pragma once

#include <unordered_map>
#include <vector>

#include "AST/Decl.hpp"

class SymbolTable {
public:
    class ScopeGuard {
        SymbolTable& table_;

    public:
        explicit ScopeGuard(SymbolTable& table)
            : table_(table) {
            table_.enter_scope();
        }
        ~ScopeGuard() { table_.exit_scope(); }

        // Non-copyable, non-movable
        ScopeGuard(const ScopeGuard&) = delete;
        ScopeGuard& operator=(const ScopeGuard&) = delete;
        ScopeGuard(ScopeGuard&&) = delete;
        ScopeGuard& operator=(ScopeGuard&&) = delete;
    };

    void enter_scope() { scopes_.emplace_back(); }

    void exit_scope() { scopes_.pop_back(); }

    bool insert_decl(Decl* decl) {
        if (lookup_decl(decl->get_id())) {
            return false; // Already exists in current scope
        }
        scopes_.back()[decl->get_id()] = decl;
        return true;
    }

    Decl* lookup_decl(const std::string& name) {
        for (int i = scopes_.size() - 1; i >= 0; --i) {
            if (auto it = scopes_[i].find(name); it != scopes_[i].end()) {
                return it->second;
            }
        }
        return nullptr;
    }

private:
    std::vector<std::unordered_map<std::string, Decl*>> scopes_;
};
