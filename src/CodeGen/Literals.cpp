#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::IntLiteral &expr) {
  curValue = llvm::ConstantInt::get(builder.getInt64Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::FloatLiteral &expr) {
  curValue = llvm::ConstantFP::get(builder.getDoubleTy(), expr.getValue());
}

void phi::CodeGen::visit(phi::StrLiteral &expr) {
  curValue = builder.CreateGlobalString(expr.getValue());
}

void phi::CodeGen::visit(phi::CharLiteral &expr) {
  curValue = llvm::ConstantInt::get(builder.getInt8Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::BoolLiteral &expr) {
  curValue = llvm::ConstantInt::get(builder.getInt1Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::RangeLiteral &expr) {
  // TODO: Implement range literals
  (void)expr; // Suppress unused parameter warning
}
