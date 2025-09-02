#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include <cassert>

void phi::CodeGen::generateFloatOp(llvm::Value *lhs, llvm::Value *rhs,
                                   phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::Plus:
    CurValue = Builder.CreateFAdd(lhs, rhs);
    break;
  case TokenKind::Minus:
    CurValue = Builder.CreateFSub(lhs, rhs);
    break;
  case TokenKind::Star:
    CurValue = Builder.CreateFMul(lhs, rhs);
    break;
  case TokenKind::Slash:
    CurValue = Builder.CreateFDiv(lhs, rhs);
    break;
  case TokenKind::Percent:
    CurValue = Builder.CreateFRem(lhs, rhs);
    break;
  case TokenKind::OpenCaret:
    CurValue = Builder.CreateFCmpOLT(lhs, rhs);
    break;
  case TokenKind::CloseCaret:
    CurValue = Builder.CreateFCmpOGT(lhs, rhs);
    break;
  case TokenKind::LessEqual:
    CurValue = Builder.CreateFCmpOLE(lhs, rhs);
    break;
  case TokenKind::GreaterEqual:
    CurValue = Builder.CreateFCmpOGE(lhs, rhs);
    break;
  case TokenKind::DoubleEquals:
    CurValue = Builder.CreateFCmpOEQ(lhs, rhs);
    break;
  case TokenKind::BangEquals:
    CurValue = Builder.CreateFCmpONE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported float operation");
  }
}

void phi::CodeGen::generateSintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::Plus:
    CurValue = Builder.CreateAdd(lhs, rhs);
    break;
  case TokenKind::Minus:
    CurValue = Builder.CreateSub(lhs, rhs);
    break;
  case TokenKind::Star:
    CurValue = Builder.CreateMul(lhs, rhs);
    break;
  case TokenKind::Slash:
    CurValue = Builder.CreateSDiv(lhs, rhs);
    break;
  case TokenKind::Percent:
    CurValue = Builder.CreateSRem(lhs, rhs);
    break;
  case TokenKind::OpenCaret:
    CurValue = Builder.CreateICmpSLT(lhs, rhs);
    break;
  case TokenKind::CloseCaret:
    CurValue = Builder.CreateICmpSGT(lhs, rhs);
    break;
  case TokenKind::LessEqual:
    CurValue = Builder.CreateICmpSLE(lhs, rhs);
    break;
  case TokenKind::GreaterEqual:
    CurValue = Builder.CreateICmpSGE(lhs, rhs);
    break;
  case TokenKind::DoubleEquals:
    CurValue = Builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenKind::BangEquals:
    CurValue = Builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported binary operation");
  }
}

void phi::CodeGen::generateUintOp(llvm::Value *lhs, llvm::Value *rhs,
                                  phi::BinaryOp &expr) {
  switch (expr.getOp()) {
  case TokenKind::Plus:
    CurValue = Builder.CreateAdd(lhs, rhs);
    break;
  case TokenKind::Minus:
    CurValue = Builder.CreateSub(lhs, rhs);
    break;
  case TokenKind::Star:
    CurValue = Builder.CreateMul(lhs, rhs);
    break;
  case TokenKind::Slash:
    CurValue = Builder.CreateUDiv(lhs, rhs);
    break;
  case TokenKind::Percent:
    CurValue = Builder.CreateURem(lhs, rhs);
    break;
  case TokenKind::OpenCaret:
    CurValue = Builder.CreateICmpULT(lhs, rhs);
    break;
  case TokenKind::CloseCaret:
    CurValue = Builder.CreateICmpUGT(lhs, rhs);
    break;
  case TokenKind::LessEqual:
    CurValue = Builder.CreateICmpULE(lhs, rhs);
    break;
  case TokenKind::GreaterEqual:
    CurValue = Builder.CreateICmpUGE(lhs, rhs);
    break;
  case TokenKind::DoubleEquals:
    CurValue = Builder.CreateICmpEQ(lhs, rhs);
    break;
  case TokenKind::BangEquals:
    CurValue = Builder.CreateICmpNE(lhs, rhs);
    break;
  default:
    throw std::runtime_error("Unsupported uint operation");
  }
}

void phi::CodeGen::visit(phi::BinaryOp &expr) {
  // Handle assignment operations separately
  if (expr.getOp() == TokenKind::Equals) {
    // For assignment, we need the pointer to the left-hand side variable
    // Don't load the value - we need the allocation pointer
    auto decl_ref = llvm::dyn_cast<DeclRefExpr>(&expr.getLhs());
    if (!decl_ref) {
      throw std::runtime_error(
          "Left-hand side of assignment must be a variable");
    }

    // Get the pointer to the variable allocation
    auto it = Decls.find(decl_ref->getDecl());
    if (it == Decls.end()) {
      throw std::runtime_error("Variable not found in declarations");
    }
    llvm::Value *lhs_ptr = it->second;

    // Evaluate right-hand side normally
    expr.getRhs().accept(*this);
    llvm::Value *rhs = CurValue;

    // Store the value
    CurValue = Builder.CreateStore(rhs, lhs_ptr);
    return;
  }

  // For all other operations, evaluate both operands normally
  // Evaluate left operand
  expr.getLhs().accept(*this);
  llvm::Value *lhs = CurValue;

  // Evaluate right operand
  expr.getRhs().accept(*this);
  llvm::Value *rhs = CurValue;

  // For comparison operations, use operand types to determine which comparison
  // to use For arithmetic operations, use result type
  Type operand_type = expr.getLhs().getType();

  if (operand_type.isFloat()) {
    generateFloatOp(lhs, rhs, expr);
    return;
  }

  if (operand_type.isSignedInteger()) {
    generateSintOp(lhs, rhs, expr);
    return;
  }

  if (operand_type.isUnsignedInteger()) {
    generateUintOp(lhs, rhs, expr);
    return;
  }
}
