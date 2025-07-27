#include "Sema/Sema.hpp"

#include <cassert>
#include <cstddef>
#include <print>

namespace phi {

// Literal expression resolution
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

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool Sema::visit(DeclRefExpr& expr) { return resolve_decl_ref(&expr, false); }

/**
 * Resolves a function call expression.
 *
 * Validates:
 * - Callee is a function
 * - Argument count matches parameter count
 * - Argument types match parameter types
 */
bool Sema::visit(FunCallExpr& expr) { return resolve_function_call(&expr); }

/**
 * Resolves a range literal expression.
 *
 * Validates:
 * - Start and end expressions resolve
 * - Both are integer types
 * - Both have same integer type
 */
bool Sema::visit(RangeLiteral& expr) {
    // Resolve start and end expressions
    if (!expr.get_start().accept(*this) || !expr.get_end().accept(*this)) {
        return false;
    }

    const Type start_type = expr.get_start().get_type();
    const Type end_type = expr.get_end().get_type();

    // Validate integer types
    if (!start_type.is_primitive() || !is_integer_type(start_type)) {
        std::println("Range start must be an integer type (i8, i16, i32, i64, u8, u16, u32, u64)");
        return false;
    }

    if (!end_type.is_primitive() || !is_integer_type(end_type)) {
        std::println("Range end must be an integer type (i8, i16, i32, i64, u8, u16, u32, u64)");
        return false;
    }

    // Validate same integer type
    if (start_type != end_type) {
        std::println("Range start and end must be the same integer type");
        return false;
    }

    expr.set_type(Type(Type::Primitive::range));
    return true;
}

/**
 * Resolves a binary operation expression.
 *
 * Validates:
 * - Both operands resolve successfully
 * - Operation is supported for operand types
 * - Type promotion rules are followed
 */
bool Sema::visit(BinaryOp& expr) {
    // Resolve operands
    if (!expr.get_lhs().accept(*this) || !expr.get_rhs().accept(*this)) {
        return false;
    }

    const Type lhs_type = expr.get_lhs().get_type();
    const Type rhs_type = expr.get_rhs().get_type();
    const TokenType op = expr.get_op();

    // Only primitive types supported
    if (!lhs_type.is_primitive() || !rhs_type.is_primitive()) {
        std::println("Binary operations not supported on custom types");
        return false;
    }

    // Operation-specific validation
    switch (op) {
        // Arithmetic operations
        case TokenType::tok_add:
        case TokenType::tok_sub:
        case TokenType::tok_mul:
        case TokenType::tok_div:
        case TokenType::tok_mod: {
            if (!is_numeric_type(lhs_type) || !is_numeric_type(rhs_type)) {
                std::println("Arithmetic operations require numeric types");
                return false;
            }
            if (op == TokenType::tok_mod &&
                (!is_integer_type(lhs_type) || !is_integer_type(rhs_type))) {
                std::println("Modulo operation requires integer types");
                return false;
            }
            // Apply type promotion
            expr.set_type(promote_numeric_types(lhs_type, rhs_type));
            break;
        }

        // Comparison operations
        case TokenType::tok_equal:
        case TokenType::tok_not_equal:
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
            if (!is_numeric_type(lhs_type) || !is_numeric_type(rhs_type) || lhs_type != rhs_type) {
                std::println("Ordering comparisons require same numeric types");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        // Logical operations
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

/**
 * Checks if type is an integer primitive.
 */
bool Sema::is_integer_type(const Type& type) {
    if (!type.is_primitive()) return false;

    const Type::Primitive prim = type.primitive_type();
    return prim == Type::Primitive::i8 || prim == Type::Primitive::i16 ||
           prim == Type::Primitive::i32 || prim == Type::Primitive::i64 ||
           prim == Type::Primitive::u8 || prim == Type::Primitive::u16 ||
           prim == Type::Primitive::u32 || prim == Type::Primitive::u64;
}

/**
 * Checks if type is numeric (integer or float).
 */
bool Sema::is_numeric_type(const Type& type) {
    if (!type.is_primitive()) return false;

    const Type::Primitive prim = type.primitive_type();
    return is_integer_type(type) || prim == Type::Primitive::f32 || prim == Type::Primitive::f64;
}

/**
 * Promotes numeric types for binary operations.
 *
 * Current rules:
 * - Same type: return type
 * - Integer + float: promote to float
 * - Mixed integers: promote to larger type (placeholder)
 */
Type Sema::promote_numeric_types(const Type& lhs, const Type& rhs) {
    if (lhs == rhs) return lhs;

    if (is_integer_type(lhs) && (rhs.primitive_type() == Type::Primitive::f32 ||
                                 rhs.primitive_type() == Type::Primitive::f64)) {
        return rhs;
    }
    if (is_integer_type(rhs) && (lhs.primitive_type() == Type::Primitive::f32 ||
                                 lhs.primitive_type() == Type::Primitive::f64)) {
        return lhs;
    }

    return lhs; // Default to left type
}

/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool Sema::visit(UnaryOp& expr) {
    // Resolve operand
    if (!expr.get_operand().accept(*this)) {
        return false;
    }

    const Type operand_type = expr.get_operand().get_type();
    const TokenType op = expr.get_op();

    // Only primitive types supported
    if (!operand_type.is_primitive()) {
        std::println("Unary operations not supported on custom types");
        return false;
    }

    // Operation-specific validation
    switch (op) {
        // Arithmetic negation
        case TokenType::tok_sub:
            if (!is_numeric_type(operand_type)) {
                std::println("Arithmetic negation requires numeric type");
                return false;
            }
            expr.set_type(operand_type);
            break;

        // Logical NOT
        case TokenType::tok_bang:
            if (operand_type.primitive_type() != Type::Primitive::boolean) {
                std::println("Logical NOT requires boolean type");
                return false;
            }
            expr.set_type(Type(Type::Primitive::boolean));
            break;

        // Increment/Decrement
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

/**
 * Resolves a declaration reference.
 *
 * @param declref Reference to resolve
 * @param function_call Whether this is part of a function call
 * @return true if resolution succeeded, false otherwise
 *
 * Handles:
 * - Variable references
 * - Function references (only when function_call = true)
 */
bool Sema::resolve_decl_ref(DeclRefExpr* declref, const bool function_call) {
    Decl* decl = symbol_table.lookup_decl(declref->get_id());

    if (!decl) {
        std::println("error: undeclared identifier '{}'", declref->get_id());
        return false;
    }

    // Prevent using function as value
    if (!function_call && dynamic_cast<FunDecl*>(decl)) {
        std::println("error: did you mean to call a function");
        return false;
    }

    declref->set_decl(decl);
    declref->set_type(decl->get_type());

    return true;
}

/**
 * Resolves a function call.
 *
 * Validates:
 * - Callee is a function
 * - Argument count matches parameter count
 * - Argument types match parameter types
 */
bool Sema::resolve_function_call(FunCallExpr* call) {
    // Resolve callee (must be function reference)
    const auto declref = dynamic_cast<DeclRefExpr*>(&call->get_callee());
    if (!resolve_decl_ref(declref, true)) {
        std::println("error: cannot call an expression");
        return false;
    }

    // Get resolved function
    const auto resolved_fun = dynamic_cast<FunDecl*>(declref->get_decl());
    assert(resolved_fun);

    // Validate argument count
    if (call->get_args().size() != resolved_fun->get_params().size()) {
        std::println("error: parameter list length mismatch");
        return false;
    }

    // Resolve arguments and validate types
    for (size_t i = 0; i < call->get_args().size(); i++) {
        Expr* arg = call->get_args()[i].get();
        if (!arg->accept(*this)) {
            std::println("error: could not resolve argument");
            return false;
        }

        ParamDecl* param = resolved_fun->get_params().at(i).get();
        if (arg->get_type() != param->get_type()) {
            std::println("error: argument type mismatch");
            return false;
        }
    }

    call->set_type(resolved_fun->get_return_type());
    return true;
}

} // namespace phi
