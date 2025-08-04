#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include <cassert>
#include <print>

void phi::CodeGen::generateFloatOp(llvm::Value *lhs, llvm::Value *rhs,
                                   phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenType::tokPlus:
    curValue = builder.CreateFAdd(lhs, rhs);
    break;
  case TokenType::tokMinus:
    curValue = builder.CreateFSub(lhs, rhs);
    break;
  case TokenType::tokStar:
    curValue = builder.CreateFMul(lhs, rhs);
    break;
  case TokenType::tokSlash:
    curValue = builder.CreateFDiv(lhs, rhs);
    break;
  case TokenType::tokPercent:
    curValue = builder.CreateFRem(lhs, rhs);
    break;
  case TokenType::tokLeftCaret:
    curValue = builder.CreateFCmpOLT(lhs, rhs);
    break;
  case TokenType::tokRightCaret:
    curValue = builder.CreateFCmpOGT(lhs, rhs);
    break;
  case TokenType::tokLessEqual:
    curValue = builder.CreateFCmpOLE(lhs, rhs);
    break;
  case TokenType::tokGreaterEqual:
    curValue = builder.CreateFCmpOGE(lhs, rhs);
    break;
  case TokenType::tokDoubleEquals:
    curValue = builder.CreateFCmpOEQ(lhs, rhs);
    break;
  case TokenType::tokBangEquals:
    curValue = builder.CreateFCmpONE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported float operation");
  }
}

void phi::CodeGen::generateSintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenType::tokPlus:
    curValue = builder.CreateAdd(lhs, rhs);
    break;
  case TokenType::tokMinus:
    curValue = builder.CreateSub(lhs, rhs);
    break;
  case TokenType::tokStar:
    curValue = builder.CreateMul(lhs, rhs);
    break;
  case TokenType::tokSlash:
    curValue = builder.CreateSDiv(lhs, rhs);
    break;
  case TokenType::tokPercent:
    curValue = builder.CreateSRem(lhs, rhs);
    break;

  case TokenType::tokLeftCaret:
    curValue = builder.CreateICmpSLT(lhs, rhs);
    break;
  case TokenType::tokRightCaret:
    curValue = builder.CreateICmpSGT(lhs, rhs);
    break;
  case TokenType::tokLessEqual:
    curValue = builder.CreateICmpSLE(lhs, rhs);
    break;
  case TokenType::tokGreaterEqual:
    curValue = builder.CreateICmpSGE(lhs, rhs);
    break;
  case TokenType::tokDoubleEquals:
    curValue = builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenType::tokBangEquals:
    curValue = builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported binary operation");
  }
}

void phi::CodeGen::generateUintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenType::tokPlus:
    curValue = builder.CreateAdd(lhs, rhs);
    break;
  case TokenType::tokMinus:
    curValue = builder.CreateSub(lhs, rhs);
    break;
  case TokenType::tokStar:
    curValue = builder.CreateMul(lhs, rhs);
    break;
  case TokenType::tokSlash:
    curValue = builder.CreateUDiv(lhs, rhs);
    break;
  case TokenType::tokPercent:
    curValue = builder.CreateURem(lhs, rhs);
    break;
  case TokenType::tokLeftCaret:
    curValue = builder.CreateICmpULT(lhs, rhs);
    break;
  case TokenType::tokRightCaret:
    curValue = builder.CreateICmpUGT(lhs, rhs);
    break;
  case TokenType::tokLessEqual:
    curValue = builder.CreateICmpULE(lhs, rhs);
    break;
  case TokenType::tokGreaterEqual:
    curValue = builder.CreateICmpUGE(lhs, rhs);
    break;
  case TokenType::tokDoubleEquals:
    curValue = builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenType::tokBangEquals:
    curValue = builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported uint operation");
  }
}

void phi::CodeGen::visit(phi::BinaryOp &expr) {
  // Handle assignment operations separately
  if (expr.getOp() == TokenType::tokEquals) {
    // For assignment, we need the pointer to the left-hand side variable
    // Don't load the value - we need the allocation pointer
    auto *decl_ref = dynamic_cast<DeclRefExpr *>(&expr.getLhs());
    if (!decl_ref) {
      throw std::runtime_error(
          "Left-hand side of assignment must be a variable");
    }

    // Get the pointer to the variable allocation
    auto it = decls.find(decl_ref->getDecl());
    if (it == decls.end()) {
      throw std::runtime_error("Variable not found in declarations");
    }
    llvm::Value *lhs_ptr = it->second;

    // Evaluate right-hand side normally
    expr.getRhs().accept(*this);
    llvm::Value *rhs = curValue;

    // Store the value
    curValue = builder.CreateStore(rhs, lhs_ptr);
    return;
  }

  // For all other operations, evaluate both operands normally
  // Evaluate left operand
  expr.getLhs().accept(*this);
  llvm::Value *lhs = curValue;

  // Evaluate right operand
  expr.getRhs().accept(*this);
  llvm::Value *rhs = curValue;

  // For comparison operations, use operand types to determine which comparison
  // to use For arithmetic operations, use result type
  Type operand_type = expr.getLhs().getTy();

  if (isFloat(operand_type)) {
    generateFloatOp(lhs, rhs, expr);
    return;
  }

  if (isSignedInt(operand_type)) {
    generateSintOp(lhs, rhs, expr);
    return;
  }

  if (isUnsignedInt(operand_type)) {
    generateUintOp(lhs, rhs, expr);
    return;
  }
}
