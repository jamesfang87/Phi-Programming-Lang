#include "Sema/Sema.hpp"
#include <cassert>
#include <print>

// ASTVisitor implementation - Expression visitors
bool Sema::visit(IntLiteral& expr) {
    expr.set_type(Type(Type::Primitive::i64));
    return true;
}

bool Sema::visit(FloatLiteral& expr) {
    expr.set_type(Type(Type::Primitive::f64));
    return true;
}

bool Sema::visit(StrLiteral& expr) {
    expr.set_type(Type(Type::Primitive::str));
    return true;
}

bool Sema::visit(CharLiteral& expr) {
    expr.set_type(Type(Type::Primitive::character));
    return true;
}

bool Sema::visit(DeclRefExpr& expr) { return resolve_decl_ref(&expr, false); }

bool Sema::visit(FunCallExpr& expr) { return resolve_function_call(&expr); }

bool Sema::visit(RangeLiteral& expr) {
    (void)expr; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::visit(BinaryOp& expr) {
    (void)expr; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::visit(UnaryOp& expr) {
    (void)expr; // suppress unused parameter warning
    // TODO: Implement actual resolution
    return true;
}

bool Sema::resolve_decl_ref(DeclRefExpr* declref, bool function_call) {
    Decl* decl = lookup_decl(declref->get_id());

    // if the declaration is not found
    if (!decl) {
        // throw error
        std::println("undeclared error: {}", declref->get_id());
        return false;
    }

    // check if the decls match ie:
    // if we are not trying to call a function and the declaration is a function
    if (!function_call && dynamic_cast<FunDecl*>(decl)) {
        std::println("Did you mean to call a function?");
        return false;
    }

    declref->set_decl(decl);
    declref->set_type(decl->get_type());

    return true;
}

bool Sema::resolve_function_call(FunCallExpr* call) {
    auto declref = dynamic_cast<DeclRefExpr*>(call->get_callee());
    bool success = resolve_decl_ref(declref, true);

    if (!success) {
        std::println("You cannot call an expression");
        return false;
    }

    // Get the resolved function declaration from the decl_ref
    auto resolved_fun = static_cast<FunDecl*>(declref->get_decl());
    assert(resolved_fun);

    // check param list length is the same
    const std::vector<std::unique_ptr<ParamDecl>>& params = resolved_fun->get_params();
    if (call->get_args().size() != params.size()) {
        std::println("Parameter list length mismatch");
        return false;
    }

    // Resolve the arguments and check param types are compatible
    for (int i = 0; i < (int)call->get_args().size(); i++) {
        // we first try to get the argument from the call
        Expr* arg = call->get_args()[i].get();
        bool success_expr = arg->accept(*this);
        if (!success_expr) {
            std::println("Could not resolve argument");
            return false;
        }

        // now we look at the parameter and see if types match
        ParamDecl* param = params.at(i).get();
        if (arg->get_type() != param->get_type()) {
            std::println("Argument type mismatch");
            return false;
        }
    }
    call->set_type(resolved_fun->get_return_type());

    return true;
}
