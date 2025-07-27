#include "Sema/Sema.hpp"

#include <cassert>
#include <cstddef>
#include <print>

namespace phi {

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

bool Sema::visit(BoolLiteral& expr) {
    expr.set_type(Type(Type::Primitive::boolean));
    return true;
}

bool Sema::visit(DeclRefExpr& expr) { return resolve_decl_ref(&expr, false); }

bool Sema::visit(FunCallExpr& expr) { return resolve_function_call(&expr); }

bool Sema::visit(RangeLiteral& expr) {
    // Resolve start and end expressions
    if (!expr.get_start().accept(*this)) {
        return false;
    }
    if (!expr.get_end().accept(*this)) {
        return false;
    }

    const Type start_type = expr.get_start().get_type();
    const Type end_type = expr.get_end().get_type();

    // Both start and end must be integer types
    if (!start_type.is_primitive() || !is_integer_type(start_type)) {
        std::println("Range start must be an integer type (i8, i16, i32, i64, u8, u16, u32, u64)");
        return false;
    }

    if (!end_type.is_primitive() || !is_integer_type(end_type)) {
        std::println("Range end must be an integer type (i8, i16, i32, i64, u8, u16, u32, u64)");
        return false;
    }

    // Start and end must be the same integer type
    if (start_type != end_type) {
        std::println("Range start and end must be the same integer type");
        return false;
    }

    // Set the range type to the integer type of its bounds
    expr.set_type(Type(Type::Primitive::range));
    return true;
}

bool Sema::visit(BinaryOp& expr) {
    // Resolve left and right operands first
    if (!expr.get_lhs().accept(*this)) {
        return false;
    }
    if (!expr.get_rhs().accept(*this)) {
        return false;
    }

    const Type lhs_type = expr.get_lhs().get_type();
    const Type rhs_type = expr.get_rhs().get_type();
    const TokenType op = expr.get_op();

    // Check if both operands are primitive types
    if (!lhs_type.is_primitive() || !rhs_type.is_primitive()) {
        std::println("Binary operations not supported on custom types");
        return false;
    }

    // Type checking based on operation
    switch (op) {
        // Arithmetic operations: +, -, *, /, %
        case TokenType::tok_add:
        case TokenType::tok_sub:
        case TokenType::tok_mul:
        case TokenType::tok_div:
        case TokenType::tok_mod: {
            if (!is_numeric_type(lhs_type) || !is_numeric_type(rhs_type)) {
                std::println("Arithmetic operations require numeric types");
                return false;
            }
            // For modulo, operands must be integers
            if (op == TokenType::tok_mod &&
                (!is_integer_type(lhs_type) || !is_integer_type(rhs_type))) {
                std::println("Modulo operation requires integer types");
                return false;
            }
            // Result type is the promoted type of the operands
            const Type result_type = promote_numeric_types(lhs_type, rhs_type);
            expr.set_type(result_type);
            break;
        }

        // Comparison operations: ==, !=, <, <=, >, >=
        case TokenType::tok_equal:
        case TokenType::tok_not_equal:
            // Equality can be checked on any primitive types if they're the same
            if (lhs_type != rhs_type) {
                std::println("Equality comparison requires same types");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        case TokenType::tok_less:
        case TokenType::tok_less_equal:
        case TokenType::tok_greater:
        case TokenType::tok_greater_equal:
            // Ordering comparisons only work on numeric types
            if (!is_numeric_type(lhs_type) || !is_numeric_type(rhs_type)) {
                std::println("Ordering comparisons require numeric types");
                return false;
            }
            if (lhs_type != rhs_type) {
                std::println("Ordering comparison requires same types");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        // Logical operations: &&, ||
        case TokenType::tok_and:
        case TokenType::tok_or:
            if (lhs_type.primitive_type() != Type::Primitive::boolean ||
                rhs_type.primitive_type() != Type::Primitive::boolean) {
                std::println("Logical operations require boolean types");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        default: std::println("Unsupported binary operation"); return false;
    }

    return true;
}

// Helper functions for type checking
bool Sema::is_integer_type(const Type& type) {
    if (!type.is_primitive()) {
        return false;
    }

    const Type::Primitive prim = type.primitive_type();
    return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
           prim == Type::Primitive::i32 || prim == Type::Primitive::i64 ||
           prim == Type::Primitive::u8 || prim == Type::Primitive::u16 ||
           prim == Type::Primitive::u32 || prim == Type::Primitive::u64;
}

bool Sema::is_numeric_type(const Type& type) {
    if (!type.is_primitive()) {
        return false;
    }

    const Type::Primitive prim = type.primitive_type();
    return is_integer_type(type) || prim == Type::Primitive::f32 || prim == Type::Primitive::f64;
}

Type Sema::promote_numeric_types(const Type& lhs, const Type& rhs) {
    // Simple type promotion rules
    // For now, if types are the same, return that type
    // In the future, implement proper numeric promotion rules
    if (lhs == rhs) {
        return lhs;
    }

    // For mixed integer/float operations, promote to float
    if (is_integer_type(lhs) && (rhs.primitive_type() == Type::Primitive::f32 ||
                                 rhs.primitive_type() == Type::Primitive::f64)) {
        return rhs;
    }
    if (is_integer_type(rhs) && (lhs.primitive_type() == Type::Primitive::f32 ||
                                 lhs.primitive_type() == Type::Primitive::f64)) {
        return lhs;
    }

    // Default to left type for now (could be improved)
    return lhs;
}

bool Sema::visit(UnaryOp& expr) {
    // Resolve operand first
    if (!expr.get_operand().accept(*this)) {
        return false;
    }

    const Type operand_type = expr.get_operand().get_type();
    const TokenType op = expr.get_op();

    // Check if operand is primitive type
    if (!operand_type.is_primitive()) {
        std::println("Unary operations not supported on custom types");
        return false;
    }

    switch (op) {
        // Arithmetic negation: -
        case TokenType::tok_sub:
            if (!is_numeric_type(operand_type)) {
                std::println("Arithmetic negation requires numeric type");
                return false;
            }
            // Negation preserves the operand type
            expr.set_type(operand_type);
            break;

        // Logical NOT: !
        case TokenType::tok_bang:
            if (operand_type.primitive_type() != Type::Primitive::boolean) {
                std::println("Logical NOT requires boolean type");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        // Increment/Decrement: ++, --
        case TokenType::tok_increment:
        case TokenType::tok_decrement:
            if (!is_integer_type(operand_type)) {
                std::println("Increment/decrement requires integer type");
                return false;
            }
            expr.set_type(operand_type);
            break;

        default: std::println("Unsupported unary operation"); return false;
    }

    return true;
}

bool Sema::resolve_decl_ref(DeclRefExpr* declref, const bool function_call) {
    Decl* decl = symbol_table.lookup_decl(declref->get_id());

    // if the declaration is not found
    if (!decl) {
        // throw error
        std::println("error: undeclared identifier '{}'", declref->get_id());
        return false;
    }

    // check if the decls match ie:
    // if we are not trying to call a function and the declaration is a function
    if (!function_call && dynamic_cast<FunDecl*>(decl)) {
        std::println("error: did you mean to call a function");
        return false;
    }

    declref->set_decl(decl);
    declref->set_type(decl->get_type());

    return true;
}

bool Sema::resolve_function_call(FunCallExpr* call) {
    const auto declref = dynamic_cast<DeclRefExpr*>(&call->get_callee());
    bool success = resolve_decl_ref(declref, true);

    if (!success) {
        std::println("error: cannot call an expression");
        return false;
    }

    // Get the resolved function declaration from the decl_ref
    const auto resolved_fun = dynamic_cast<FunDecl*>(declref->get_decl());
    assert(resolved_fun);

    // check param list length is the same
    const std::vector<std::unique_ptr<ParamDecl>>& params = resolved_fun->get_params();
    if (call->get_args().size() != params.size()) {
        std::println("error: parameter list length mismatch");
        return false;
    }

    // Resolve the arguments and check param types are compatible
    for (size_t i = 0; i < call->get_args().size(); i++) {
        // we first try to get the argument from the call
        Expr* arg = call->get_args()[i].get();
        if (const bool success_expr = arg->accept(*this); !success_expr) {
            std::println("error: could not resolve argument");
            return false;
        }

        // now we look at the parameter and see if types match
        ParamDecl* param = params.at(i).get();
        if (arg->get_type() != param->get_type()) {
            std::println("error: argument type mismatch");
            return false;
        }
    }
    call->set_type(resolved_fun->get_return_type());

    return true;
}

} // namespace phi
