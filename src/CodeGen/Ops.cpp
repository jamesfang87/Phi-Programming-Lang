#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::generate_float_op(llvm::Value* lhs, llvm::Value* rhs, phi::BinaryOp& expr) {
    switch (expr.get_op()) {
        case TokenType::tok_add: current_value = builder.CreateFAdd(lhs, rhs); break;
        case TokenType::tok_sub: current_value = builder.CreateFSub(lhs, rhs); break;
        case TokenType::tok_mul: current_value = builder.CreateFMul(lhs, rhs); break;
        case TokenType::tok_div: current_value = builder.CreateFDiv(lhs, rhs); break;
        case TokenType::tok_mod: current_value = builder.CreateFRem(lhs, rhs); break;
        case TokenType::tok_less: current_value = builder.CreateFCmpOLT(lhs, rhs); break;
        case TokenType::tok_greater: current_value = builder.CreateFCmpOGT(lhs, rhs); break;
        case TokenType::tok_less_equal: current_value = builder.CreateFCmpOLE(lhs, rhs); break;
        case TokenType::tok_greater_equal: current_value = builder.CreateFCmpOGE(lhs, rhs); break;
        case TokenType::tok_equal: current_value = builder.CreateFCmpOEQ(lhs, rhs); break;
        case TokenType::tok_not_equal: current_value = builder.CreateFCmpONE(lhs, rhs); break;
        default: throw std::runtime_error("Unsupported float operation");
    }
}

void phi::CodeGen::generate_sint_op(llvm::Value* lhs, llvm::Value* rhs, phi::BinaryOp& expr) {
    switch (expr.get_op()) {
        case TokenType::tok_add: current_value = builder.CreateAdd(lhs, rhs); break;
        case TokenType::tok_sub: current_value = builder.CreateSub(lhs, rhs); break;
        case TokenType::tok_mul: current_value = builder.CreateMul(lhs, rhs); break;
        case TokenType::tok_div: current_value = builder.CreateSDiv(lhs, rhs); break;
        case TokenType::tok_mod: current_value = builder.CreateSRem(lhs, rhs); break;
        case TokenType::tok_less: current_value = builder.CreateICmpSLT(lhs, rhs); break;
        case TokenType::tok_greater: current_value = builder.CreateICmpSGT(lhs, rhs); break;
        case TokenType::tok_less_equal: current_value = builder.CreateICmpSLE(lhs, rhs); break;
        case TokenType::tok_greater_equal: current_value = builder.CreateICmpSGE(lhs, rhs); break;
        case TokenType::tok_equal: current_value = builder.CreateICmpEQ(lhs, rhs); break;
        case TokenType::tok_not_equal: current_value = builder.CreateICmpNE(lhs, rhs); break;
        default: throw std::runtime_error("Unsupported binary operation");
    }
}

void phi::CodeGen::generate_uint_op(llvm::Value* lhs, llvm::Value* rhs, phi::BinaryOp& expr) {
    switch (expr.get_op()) {
        case TokenType::tok_add: current_value = builder.CreateAdd(lhs, rhs); break;
        case TokenType::tok_sub: current_value = builder.CreateSub(lhs, rhs); break;
        case TokenType::tok_mul: current_value = builder.CreateMul(lhs, rhs); break;
        case TokenType::tok_div: current_value = builder.CreateUDiv(lhs, rhs); break;
        case TokenType::tok_mod: current_value = builder.CreateURem(lhs, rhs); break;
        case TokenType::tok_less: current_value = builder.CreateICmpULT(lhs, rhs); break;
        case TokenType::tok_greater: current_value = builder.CreateICmpUGT(lhs, rhs); break;
        case TokenType::tok_less_equal: current_value = builder.CreateICmpULE(lhs, rhs); break;
        case TokenType::tok_greater_equal: current_value = builder.CreateICmpUGE(lhs, rhs); break;
        case TokenType::tok_equal: current_value = builder.CreateICmpEQ(lhs, rhs); break;
        case TokenType::tok_not_equal: current_value = builder.CreateICmpNE(lhs, rhs); break;
        default: throw std::runtime_error("Unsupported uint operation");
    }
}

void phi::CodeGen::visit(phi::BinaryOp& expr) {
    // Evaluate left operand
    expr.get_lhs().accept(*this);
    llvm::Value* lhs = current_value;

    // Evaluate right operand
    expr.get_rhs().accept(*this);
    llvm::Value* rhs = current_value;

    if (is_float_type(expr.get_type())) {
        generate_float_op(lhs, rhs, expr);
        return;
    }

    if (is_signed_int(expr.get_type())) {
        generate_sint_op(lhs, rhs, expr);
        return;
    }

    if (is_unsigned_int(expr.get_type())) {
        generate_uint_op(lhs, rhs, expr);
        return;
    }
}
