#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::ReturnStmt& stmt) {
    if (stmt.has_expr()) {
        stmt.get_expr().accept(*this);
        builder.CreateRet(current_value);
    } else {
        builder.CreateRetVoid();
    }
}

void phi::CodeGen::visit(phi::IfStmt& stmt) {
    // TODO: Implement if statements
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::WhileStmt& stmt) {
    // TODO: Implement while loops
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::ForStmt& stmt) {
    // TODO: Implement for loops
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::LetStmt& stmt) {
    // TODO: Implement let statements
    (void)stmt; // Suppress unused parameter warning
}
