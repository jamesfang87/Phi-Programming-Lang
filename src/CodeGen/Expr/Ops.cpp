#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

#include "Lexer/TokenKind.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(BinaryOp &E) {
  if (E.getOp() == TokenKind::Equals) {
    if (auto *Lhs = llvm::dyn_cast<DeclRefExpr>(&E.getLhs())) {
      llvm::Value *Alloc = DeclMap[Lhs->getDecl()];
      llvm::Value *RhsVal = visit(E.getRhs());
      return Builder.CreateStore(RhsVal, Alloc);
    }

    throw std::runtime_error("unsupported assignment lhs");
  }

  llvm::Value *L = E.getLhs().accept(*this);
  llvm::Value *R = E.getRhs().accept(*this);

  Type OperandType = E.getLhs().getType();
  if (OperandType.isFloat()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateFAdd(L, R);
    case TokenKind::Minus:
      return Builder.CreateFSub(L, R);
    case TokenKind::Star:
      return Builder.CreateFMul(L, R);
    case TokenKind::Slash:
      return Builder.CreateFDiv(L, R);
    case TokenKind::Percent:
      return Builder.CreateFRem(L, R);
    case TokenKind::OpenCaret:
      return Builder.CreateFCmpOLT(L, R);
    case TokenKind::CloseCaret:
      return Builder.CreateFCmpOGT(L, R);
    case TokenKind::LessEqual:
      return Builder.CreateFCmpOLE(L, R);
    case TokenKind::GreaterEqual:
      return Builder.CreateFCmpOGE(L, R);
    case TokenKind::DoubleEquals:
      return Builder.CreateFCmpOEQ(L, R);
    case TokenKind::BangEquals:
      return Builder.CreateFCmpONE(L, R);
    default:
      break;
    }
  } else if (OperandType.isSignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(L, R);
    case TokenKind::Minus:
      return Builder.CreateSub(L, R);
    case TokenKind::Star:
      return Builder.CreateMul(L, R);
    case TokenKind::Slash:
      return Builder.CreateSDiv(L, R);
    case TokenKind::Percent:
      return Builder.CreateSRem(L, R);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpSLT(L, R);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpSGT(L, R);
    case TokenKind::LessEqual:
      return Builder.CreateICmpSLE(L, R);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpSGE(L, R);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(L, R);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(L, R);
    default:
      break;
    }
  } else if (OperandType.isUnsignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(L, R);
    case TokenKind::Minus:
      return Builder.CreateSub(L, R);
    case TokenKind::Star:
      return Builder.CreateMul(L, R);
    case TokenKind::Slash:
      return Builder.CreateUDiv(L, R);
    case TokenKind::Percent:
      return Builder.CreateURem(L, R);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpULT(L, R);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpUGT(L, R);
    case TokenKind::LessEqual:
      return Builder.CreateICmpULE(L, R);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpUGE(L, R);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(L, R);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(L, R);
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
