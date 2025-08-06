#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include <cassert>
#include <print>

void phi::CodeGen::generateFloatOp(llvm::Value *lhs, llvm::Value *rhs,
                                   phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::tokPlus:
    curValue = builder.CreateFAdd(lhs, rhs);
    break;
  case TokenKind::tokMinus:
    curValue = builder.CreateFSub(lhs, rhs);
    break;
  case TokenKind::tokStar:
    curValue = builder.CreateFMul(lhs, rhs);
    break;
  case TokenKind::tokSlash:
    curValue = builder.CreateFDiv(lhs, rhs);
    break;
  case TokenKind::tokPercent:
    curValue = builder.CreateFRem(lhs, rhs);
    break;
  case TokenKind::tokLeftCaret:
    curValue = builder.CreateFCmpOLT(lhs, rhs);
    break;
  case TokenKind::tokRightCaret:
    curValue = builder.CreateFCmpOGT(lhs, rhs);
    break;
  case TokenKind::tokLessEqual:
    curValue = builder.CreateFCmpOLE(lhs, rhs);
    break;
  case TokenKind::tokGreaterEqual:
    curValue = builder.CreateFCmpOGE(lhs, rhs);
    break;
  case TokenKind::tokDoubleEquals:
    curValue = builder.CreateFCmpOEQ(lhs, rhs);
    break;
  case TokenKind::tokBangEquals:
    curValue = builder.CreateFCmpONE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported float operation");
  }
}

void phi::CodeGen::generateSintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::tokPlus:
    curValue = builder.CreateAdd(lhs, rhs);
    break;
  case TokenKind::tokMinus:
    curValue = builder.CreateSub(lhs, rhs);
    break;
  case TokenKind::tokStar:
    curValue = builder.CreateMul(lhs, rhs);
    break;
  case TokenKind::tokSlash:
    curValue = builder.CreateSDiv(lhs, rhs);
    break;
  case TokenKind::tokPercent:
    curValue = builder.CreateSRem(lhs, rhs);
    break;

  case TokenKind::tokLeftCaret:
    curValue = builder.CreateICmpSLT(lhs, rhs);
    break;
  case TokenKind::tokRightCaret:
    curValue = builder.CreateICmpSGT(lhs, rhs);
    break;
  case TokenKind::tokLessEqual:
    curValue = builder.CreateICmpSLE(lhs, rhs);
    break;
  case TokenKind::tokGreaterEqual:
    curValue = builder.CreateICmpSGE(lhs, rhs);
    break;
  case TokenKind::tokDoubleEquals:
    curValue = builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenKind::tokBangEquals:
    curValue = builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported binary operation");
  }
}

void phi::CodeGen::generateUintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::tokPlus:
    curValue = builder.CreateAdd(lhs, rhs);
    break;
  case TokenKind::tokMinus:
    curValue = builder.CreateSub(lhs, rhs);
    break;
  case TokenKind::tokStar:
    curValue = builder.CreateMul(lhs, rhs);
    break;
  case TokenKind::tokSlash:
    curValue = builder.CreateUDiv(lhs, rhs);
    break;
  case TokenKind::tokPercent:
    curValue = builder.CreateURem(lhs, rhs);
    break;
  case TokenKind::tokLeftCaret:
    curValue = builder.CreateICmpULT(lhs, rhs);
    break;
  case TokenKind::tokRightCaret:
    curValue = builder.CreateICmpUGT(lhs, rhs);
    break;
  case TokenKind::tokLessEqual:
    curValue = builder.CreateICmpULE(lhs, rhs);
    break;
  case TokenKind::tokGreaterEqual:
    curValue = builder.CreateICmpUGE(lhs, rhs);
    break;
  case TokenKind::tokDoubleEquals:
    curValue = builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenKind::tokBangEquals:
    curValue = builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported uint operation");
  }
}

void phi::CodeGen::visit(phi::BinaryOp &expr) {
  // Handle assignment operations separately
  if (expr.getOp() == TokenKind::tokEquals) {
    // For assignment, we need the pointer to the left-hand side variable
    // Don't load the value - we need the allocation pointer
    auto decl_ref = llvm::dyn_cast<DeclRefExpr>(&expr.getLhs());
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
  Type operand_type = expr.getLhs().getType();

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
