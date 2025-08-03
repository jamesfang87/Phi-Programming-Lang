#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::IntLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt64Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::FloatLiteral& expr) {
    current_value = llvm::ConstantFP::get(builder.getDoubleTy(), expr.get_value());
}

void phi::CodeGen::visit(phi::StrLiteral& expr) {
    current_value = builder.CreateGlobalString(expr.get_value());
}

void phi::CodeGen::visit(phi::CharLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt8Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::BoolLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt1Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::RangeLiteral& expr) {
    // TODO: Implement range literals
    (void)expr; // Suppress unused parameter warning
}
