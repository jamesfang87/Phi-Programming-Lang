#include "Sema/Sema.hpp"
#include <print>

// ASTVisitor implementation - Statement visitors
bool Sema::visit(Block& block) {
    for (auto& stmt : block.get_stmts()) {
        if (!stmt->accept(*this)) return false;
    }
    return true;
}

bool Sema::visit(ReturnStmt& stmt) { return resolve_return_stmt(&stmt); }

bool Sema::visit(IfStmt& stmt) {
    (void)stmt; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::visit(WhileStmt& stmt) {
    (void)stmt; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::visit(ForStmt& stmt) {
    (void)stmt; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::visit(Expr& stmt) {
    return stmt.accept(*this); // Handle expression-as-statement
}

bool Sema::resolve_return_stmt(ReturnStmt* stmt) {
    // case that the function is void
    if (!stmt->get_expr()) {
        if (cur_fun->get_return_type() != Type(Type::Primitive::null)) {
            std::println("error: function '{}' should return a value", cur_fun->get_id());
            return false;
        }
        return true;
    }

    // now handle the case that the function is not void
    bool success = stmt->get_expr()->accept(*this);
    if (!success) {
        return false;
    }

    // compare to the current function
    std::println("stmt: {}, in fn: {}", stmt->get_expr()->is_resolved(), cur_fun->get_id());
    if (stmt->get_expr()->get_type() != cur_fun->get_return_type()) {
        // throw error
        std::println("type mismatch error: {}", cur_fun->get_id());
        std::println("return stmt type: {}", stmt->get_expr()->get_type().to_string());
        std::println("expected type: {}", cur_fun->get_return_type().to_string());
        return false;
    }

    return true;
}
