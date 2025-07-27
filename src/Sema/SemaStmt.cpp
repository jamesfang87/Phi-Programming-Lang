#include "Sema/Sema.hpp"

#include <print>

#include "AST/Decl.hpp"

bool Sema::visit(Block& block) {
    // check if this is a function body block
    // function body shares scope with parameters - don't create new scope
    if (!is_function_body_block) {
        // if not, then create a new scope
        SymbolTable::ScopeGuard block_scope(symbol_table);
    }

    // resolve all statements
    for (const auto& stmt : block.get_stmts()) {
        if (!stmt->accept(*this)) return false;
    }

    // scope automatically popped by ScopeGuard destructor
    return true;
}

bool Sema::visit(ReturnStmt& stmt) {
    // case that the function is void
    if (!stmt.has_expr()) {
        if (cur_fun->get_return_type() != Type(Type::Primitive::null)) {
            std::println("error: function '{}' should return a value", cur_fun->get_id());
            return false;
        }
        return true;
    }

    // now handle the case that the function is not void
    bool success = stmt.get_expr().accept(*this);
    if (!success) {
        return false;
    }

    // compare to the current function
    if (stmt.get_expr().get_type() != cur_fun->get_return_type()) {
        // throw error
        std::println("type mismatch error: {}", cur_fun->get_id());
        std::println("return stmt type: {}", stmt.get_expr().get_type().to_string());
        std::println("expected type: {}", cur_fun->get_return_type().to_string());
        return false;
    }

    return true;
}

bool Sema::visit(IfStmt& stmt) {
    // first resolve the condition
    bool res = stmt.get_condition().accept(*this);
    if (!res) return false;

    // now we check whether the condition is of type bool
    if (stmt.get_condition().get_type() != Type(Type::Primitive::boolean)) {
        std::println("error: condition in if statement must have type bool");
        return false;
    }

    // now we resolve the bodies
    if (!stmt.get_then().accept(*this)) return false;
    if (stmt.has_else()) {
        if (!stmt.get_else().accept(*this)) return false;
    }

    return true;
}

bool Sema::visit(WhileStmt& stmt) {
    bool res = stmt.get_condition().accept(*this);
    if (!res) return false;

    // now we check whether the condition is of type bool
    if (stmt.get_condition().get_type() != Type(Type::Primitive::boolean)) {
        std::println("error: condition in while statement must have type bool");
        return false;
    }

    // now we resolve the body
    if (!stmt.get_body().accept(*this)) return false;

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

bool Sema::visit(LetStmt& stmt) {
    // First resolve the variable's declared type
    VarDecl& var = stmt.get_var_decl();
    const bool type = resolve_type(var.get_type());
    if (!type) {
        std::println("invalid type for variable");
        return false;
    }

    // If there's an initializer, resolve it and check type compatibility
    if (var.has_initializer()) {
        Expr& initializer = var.get_initializer();
        if (!initializer.accept(*this)) {
            std::println("failed to resolve variable initializer");
            return false;
        }

        // Check if initializer type matches variable type
        if (initializer.get_type() != var.get_type()) {
            std::println("variable initializer type mismatch");
            std::println("variable type: {}", var.get_type().to_string());
            std::println("initializer type: {}", initializer.get_type().to_string());
            return false;
        }
    } else if (var.is_constant()) {
        // Constant variables must have an initializer
        std::println("constant variable '{}' must have an initializer", var.get_id());
        return false;
    }
    symbol_table.insert_decl(&var);

    return true;
}
