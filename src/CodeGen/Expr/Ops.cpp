#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

#include "Lexer/TokenKind.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(BinaryOp &E) {
  if (E.getOp() == TokenKind::Equals) {
    if (auto *LHS = llvm::dyn_cast<DeclRefExpr>(&E.getLhs())) {
      llvm::Value *Alloc = DeclMap[LHS->getDecl()];
      llvm::Value *RHSVal = visit(E.getRhs());
      return Builder.CreateStore(RHSVal, Alloc);
    }

    throw std::runtime_error("unsupported assignment lhs");
  }

  llvm::Value *LHS = E.getLhs().accept(*this);
  llvm::Value *RHS = E.getRhs().accept(*this);

  Type OperandType = E.getLhs().getType();
  if (OperandType.isFloat()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateFAdd(LHS, RHS);
    case TokenKind::Minus:
      return Builder.CreateFSub(LHS, RHS);
    case TokenKind::Star:
      return Builder.CreateFMul(LHS, RHS);
    case TokenKind::Slash:
      return Builder.CreateFDiv(LHS, RHS);
    case TokenKind::Percent:
      return Builder.CreateFRem(LHS, RHS);
    case TokenKind::OpenCaret:
      return Builder.CreateFCmpOLT(LHS, RHS);
    case TokenKind::CloseCaret:
      return Builder.CreateFCmpOGT(LHS, RHS);
    case TokenKind::LessEqual:
      return Builder.CreateFCmpOLE(LHS, RHS);
    case TokenKind::GreaterEqual:
      return Builder.CreateFCmpOGE(LHS, RHS);
    case TokenKind::DoubleEquals:
      return Builder.CreateFCmpOEQ(LHS, RHS);
    case TokenKind::BangEquals:
      return Builder.CreateFCmpONE(LHS, RHS);
    default:
      break;
    }
  } else if (OperandType.isSignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(LHS, RHS);
    case TokenKind::Minus:
      return Builder.CreateSub(LHS, RHS);
    case TokenKind::Star:
      return Builder.CreateMul(LHS, RHS);
    case TokenKind::Slash:
      return Builder.CreateSDiv(LHS, RHS);
    case TokenKind::Percent:
      return Builder.CreateSRem(LHS, RHS);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpSLT(LHS, RHS);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpSGT(LHS, RHS);
    case TokenKind::LessEqual:
      return Builder.CreateICmpSLE(LHS, RHS);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpSGE(LHS, RHS);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(LHS, RHS);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(LHS, RHS);
    default:
      break;
    }
  } else if (OperandType.isUnsignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(LHS, RHS);
    case TokenKind::Minus:
      return Builder.CreateSub(LHS, RHS);
    case TokenKind::Star:
      return Builder.CreateMul(LHS, RHS);
    case TokenKind::Slash:
      return Builder.CreateUDiv(LHS, RHS);
    case TokenKind::Percent:
      return Builder.CreateURem(LHS, RHS);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpULT(LHS, RHS);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpUGT(LHS, RHS);
    case TokenKind::LessEqual:
      return Builder.CreateICmpULE(LHS, RHS);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpUGE(LHS, RHS);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(LHS, RHS);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(LHS, RHS);
    default:
      break;
    }
  }

  throw std::runtime_error(
      std::format("unsupported binary operator for operands of type: {}, {}",
                  tyToStr(E.getOp()), OperandType.toString()));
}

llvm::Value *CodeGen::visit(UnaryOp &E) {
  llvm::Value *Val = E.getOperand().accept(*this);
  switch (E.getOp()) {
  case TokenKind::Minus:
    if (E.getOperand().getType().isFloat())
      return Builder.CreateFNeg(Val);
    return Builder.CreateNeg(Val);
  case TokenKind::Bang:
    if (!Val->getType()->isIntegerTy(1))
      Val =
          Builder.CreateICmpNE(Val, llvm::ConstantInt::get(Val->getType(), 0));
    return Builder.CreateNot(Val);
  default:
    return nullptr;
  }
}
