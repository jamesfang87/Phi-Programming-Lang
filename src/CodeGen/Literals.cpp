#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::IntLiteral &expr) {
  CurValue = llvm::ConstantInt::get(Builder.getInt64Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::FloatLiteral &expr) {
  CurValue = llvm::ConstantFP::get(Builder.getDoubleTy(), expr.getValue());
}

void phi::CodeGen::visit(phi::StrLiteral &expr) {
  CurValue = Builder.CreateGlobalString(expr.getValue());
}

void phi::CodeGen::visit(phi::CharLiteral &expr) {
  CurValue = llvm::ConstantInt::get(Builder.getInt8Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::BoolLiteral &expr) {
  CurValue = llvm::ConstantInt::get(Builder.getInt1Ty(), expr.getValue());
}

void phi::CodeGen::visit(phi::RangeLiteral &expr) {
  // TODO: Implement range literals
  (void)expr; // Suppress unused parameter warning
}
