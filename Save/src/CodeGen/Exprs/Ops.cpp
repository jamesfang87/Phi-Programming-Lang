#include "AST/Nodes/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "Lexer/TokenKind.hpp"
#include "llvm/Support/ErrorHandling.h"
#include <cassert>
#include <llvm/Support/Casting.h>

using namespace phi;

// ------------------------------------------------------------
// Helpers
// ------------------------------------------------------------

llvm::Value *CodeGen::toBoolValue(llvm::Value *Val, const Type &Ty) {
  if (Ty.isFloat()) {
    llvm::Value *Zero = llvm::ConstantFP::get(Val->getType(), 0.0);
    return Builder.CreateFCmpONE(Val, Zero, "booltmp");
  }

  llvm::Value *Zero = llvm::ConstantInt::get(Val->getType(), 0);
  return Builder.CreateICmpNE(Val, Zero, "booltmp");
}

llvm::Value *CodeGen::getLValuePointer(Expr *E) {
  if (auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(E)) {
    auto It = DeclMap.find(DeclRef->getDecl());
    return (It == DeclMap.end()) ? nullptr : It->second;
  }

  if (auto *FieldAccess = llvm::dyn_cast<FieldAccessExpr>(E))
    return visit(*FieldAccess);

  return nullptr;
}

// ------------------------------------------------------------
// Binary operations
// ------------------------------------------------------------

llvm::Value *CodeGen::emitAssignment(BinaryOp &E) {
  llvm::Value *Ptr = getLValuePointer(&E.getLhs());
  assert(Ptr && "Invalid ptr to assign to");

  llvm::Value *RhsValue = load(visit(E.getRhs()), E.getLhs().getType());
  return store(RhsValue, Ptr, E.getLhs().getType());
}

llvm::Value *CodeGen::emitCompoundAssignment(BinaryOp &E) {
  llvm::Value *Ptr = getLValuePointer(&E.getLhs());
  assert(Ptr && "Invalid ptr to assign to");

  Type Ty = E.getLhs().getType();
  llvm::Value *OldValue = load(Ptr, Ty);
  llvm::Value *RhsValue = load(visit(E.getRhs()), Ty);

  llvm::Value *UpdatedValue = nullptr;

  switch (E.getOp()) {
  case TokenKind::PlusEquals:
    UpdatedValue = Ty.isFloat() ? Builder.CreateFAdd(OldValue, RhsValue)
                                : Builder.CreateAdd(OldValue, RhsValue);
    break;
  case TokenKind::SubEquals:
    UpdatedValue = Ty.isFloat() ? Builder.CreateFSub(OldValue, RhsValue)
                                : Builder.CreateSub(OldValue, RhsValue);
    break;
  case TokenKind::MulEquals:
    UpdatedValue = Ty.isFloat() ? Builder.CreateFMul(OldValue, RhsValue)
                                : Builder.CreateMul(OldValue, RhsValue);
    break;
  case TokenKind::DivEquals:
    if (Ty.isFloat())
      UpdatedValue = Builder.CreateFDiv(OldValue, RhsValue);
    else if (Ty.isSignedInteger())
      UpdatedValue = Builder.CreateSDiv(OldValue, RhsValue);
    else if (Ty.isUnsignedInteger())
      UpdatedValue = Builder.CreateUDiv(OldValue, RhsValue);
    break;
  case TokenKind::ModEquals:
    if (Ty.isFloat())
      UpdatedValue = Builder.CreateFRem(OldValue, RhsValue);
    else if (Ty.isSignedInteger())
      UpdatedValue = Builder.CreateSRem(OldValue, RhsValue);
    else if (Ty.isUnsignedInteger())
      UpdatedValue = Builder.CreateURem(OldValue, RhsValue);
    break;
  default:
    break;
  }

  assert(UpdatedValue && "UpdatedValue should not be nullptr. "
                         "An invalid compound assigment was probably met.");
  store(UpdatedValue, Ptr, Ty);
  return UpdatedValue;
}

llvm::Value *CodeGen::emitLogicalAnd(BinaryOp &E) {
  llvm::Value *LhsVal = load(visit(E.getLhs()), E.getLhs().getType());
  llvm::Value *LhsBool = toBoolValue(LhsVal, E.getLhs().getType());

  llvm::Function *Fn = Builder.GetInsertBlock()->getParent();
  llvm::BasicBlock *LhsBB = Builder.GetInsertBlock();
  llvm::BasicBlock *RhsBB = llvm::BasicBlock::Create(Context, "and.rhs", Fn);
  llvm::BasicBlock *ContBB = llvm::BasicBlock::Create(Context, "and.cont", Fn);

  Builder.CreateCondBr(LhsBool, RhsBB, ContBB);

  Builder.SetInsertPoint(RhsBB);
  llvm::Value *RhsVal = load(visit(E.getRhs()), E.getRhs().getType());
  llvm::Value *RhsBool = toBoolValue(RhsVal, E.getRhs().getType());
  Builder.CreateBr(ContBB);

  Builder.SetInsertPoint(ContBB);
  llvm::PHINode *Phi = Builder.CreatePHI(llvm::Type::getInt1Ty(Context), 2);
  Phi->addIncoming(llvm::ConstantInt::getFalse(Context), LhsBB);
  Phi->addIncoming(RhsBool, RhsBB);
  return Phi;
}

llvm::Value *CodeGen::emitLogicalOr(BinaryOp &E) {
  llvm::Value *LhsVal = load(visit(E.getLhs()), E.getLhs().getType());
  llvm::Value *LhsBool = toBoolValue(LhsVal, E.getLhs().getType());

  llvm::Function *Fn = Builder.GetInsertBlock()->getParent();
  llvm::BasicBlock *LhsBB = Builder.GetInsertBlock();
  llvm::BasicBlock *RhsBB = llvm::BasicBlock::Create(Context, "or.rhs", Fn);
  llvm::BasicBlock *ContBB = llvm::BasicBlock::Create(Context, "or.cont", Fn);

  Builder.CreateCondBr(LhsBool, ContBB, RhsBB);

  Builder.SetInsertPoint(RhsBB);
  llvm::Value *RhsVal = load(visit(E.getRhs()), E.getRhs().getType());
  llvm::Value *RhsBool = toBoolValue(RhsVal, E.getRhs().getType());
  Builder.CreateBr(ContBB);

  Builder.SetInsertPoint(ContBB);
  llvm::PHINode *Phi = Builder.CreatePHI(llvm::Type::getInt1Ty(Context), 2);
  Phi->addIncoming(llvm::ConstantInt::getTrue(Context), LhsBB);
  Phi->addIncoming(RhsBool, RhsBB);
  return Phi;
}

llvm::Value *CodeGen::emitArithmeticOrComparison(BinaryOp &E) {
  Type Ty = E.getLhs().getType();
  llvm::Value *LhsVal = load(visit(E.getLhs()), Ty);
  llvm::Value *RhsVal = load(visit(E.getRhs()), Ty);

  if (Ty.isFloat()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateFAdd(LhsVal, RhsVal);
    case TokenKind::Minus:
      return Builder.CreateFSub(LhsVal, RhsVal);
    case TokenKind::Star:
      return Builder.CreateFMul(LhsVal, RhsVal);
    case TokenKind::Slash:
      return Builder.CreateFDiv(LhsVal, RhsVal);
    case TokenKind::Percent:
      return Builder.CreateFRem(LhsVal, RhsVal);
    case TokenKind::OpenCaret:
      return Builder.CreateFCmpOLT(LhsVal, RhsVal);
    case TokenKind::CloseCaret:
      return Builder.CreateFCmpOGT(LhsVal, RhsVal);
    case TokenKind::LessEqual:
      return Builder.CreateFCmpOLE(LhsVal, RhsVal);
    case TokenKind::GreaterEqual:
      return Builder.CreateFCmpOGE(LhsVal, RhsVal);
    case TokenKind::DoubleEquals:
      return Builder.CreateFCmpOEQ(LhsVal, RhsVal);
    case TokenKind::BangEquals:
      return Builder.CreateFCmpONE(LhsVal, RhsVal);
    default:
      break;
    }
  } else if (Ty.isSignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(LhsVal, RhsVal);
    case TokenKind::Minus:
      return Builder.CreateSub(LhsVal, RhsVal);
    case TokenKind::Star:
      return Builder.CreateMul(LhsVal, RhsVal);
    case TokenKind::Slash:
      return Builder.CreateSDiv(LhsVal, RhsVal);
    case TokenKind::Percent:
      return Builder.CreateSRem(LhsVal, RhsVal);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpSLT(LhsVal, RhsVal);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpSGT(LhsVal, RhsVal);
    case TokenKind::LessEqual:
      return Builder.CreateICmpSLE(LhsVal, RhsVal);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpSGE(LhsVal, RhsVal);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(LhsVal, RhsVal);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(LhsVal, RhsVal);
    default:
      break;
    }
  } else if (Ty.isUnsignedInteger()) {
    switch (E.getOp()) {
    case TokenKind::Plus:
      return Builder.CreateAdd(LhsVal, RhsVal);
    case TokenKind::Minus:
      return Builder.CreateSub(LhsVal, RhsVal);
    case TokenKind::Star:
      return Builder.CreateMul(LhsVal, RhsVal);
    case TokenKind::Slash:
      return Builder.CreateUDiv(LhsVal, RhsVal);
    case TokenKind::Percent:
      return Builder.CreateURem(LhsVal, RhsVal);
    case TokenKind::OpenCaret:
      return Builder.CreateICmpULT(LhsVal, RhsVal);
    case TokenKind::CloseCaret:
      return Builder.CreateICmpUGT(LhsVal, RhsVal);
    case TokenKind::LessEqual:
      return Builder.CreateICmpULE(LhsVal, RhsVal);
    case TokenKind::GreaterEqual:
      return Builder.CreateICmpUGE(LhsVal, RhsVal);
    case TokenKind::DoubleEquals:
      return Builder.CreateICmpEQ(LhsVal, RhsVal);
    case TokenKind::BangEquals:
      return Builder.CreateICmpNE(LhsVal, RhsVal);
    default:
      break;
    }
  }

  llvm_unreachable("unsupported Binary Operator");
}

llvm::Value *CodeGen::visit(BinaryOp &E) {
  if (E.getOp() == TokenKind::Equals)
    return emitAssignment(E);
  if (E.getOp() == TokenKind::PlusEquals || E.getOp() == TokenKind::SubEquals ||
      E.getOp() == TokenKind::MulEquals || E.getOp() == TokenKind::DivEquals ||
      E.getOp() == TokenKind::ModEquals)
    return emitCompoundAssignment(E);
  if (E.getOp() == TokenKind::DoubleAmp)
    return emitLogicalAnd(E);
  if (E.getOp() == TokenKind::DoublePipe)
    return emitLogicalOr(E);

  return emitArithmeticOrComparison(E);
}

// ------------------------------------------------------------
// Unary operations
// ------------------------------------------------------------

llvm::Value *CodeGen::visit(UnaryOp &E) {
  // ++/--
  if ((E.getOp() == TokenKind::DoublePlus ||
       E.getOp() == TokenKind::DoubleMinus)) {

    llvm::Value *Ptr = getLValuePointer(&E.getOperand());
    assert(Ptr && "Must be assignment to a lvalue");

    Type Ty = E.getOperand().getType();
    llvm::Value *OldValue = load(Ptr, Ty);
    llvm::Value *StepValue =
        Ty.isFloat() ? llvm::ConstantFP::get(OldValue->getType(), 1.0)
                     : llvm::ConstantInt::get(OldValue->getType(), 1);

    llvm::Value *UpdatedValue =
        (E.getOp() == TokenKind::DoublePlus)
            ? (Ty.isFloat() ? Builder.CreateFAdd(OldValue, StepValue)
                            : Builder.CreateAdd(OldValue, StepValue))
            : (Ty.isFloat() ? Builder.CreateFSub(OldValue, StepValue)
                            : Builder.CreateSub(OldValue, StepValue));

    store(UpdatedValue, Ptr, Ty);
    return (E.isPrefixOp()) ? UpdatedValue : OldValue;
  }

  // unary minus
  if (E.getOp() == TokenKind::Minus) {
    llvm::Value *Val = load(visit(E.getOperand()), E.getOperand().getType());
    return E.getOperand().getType().isFloat() ? Builder.CreateFNeg(Val)
                                              : Builder.CreateNeg(Val);
  }

  // logical not
  if (E.getOp() == TokenKind::Bang) {
    llvm::Value *Val = load(visit(E.getOperand()), E.getOperand().getType());
    if (!Val->getType()->isIntegerTy(1))
      Val =
          Builder.CreateICmpNE(Val, llvm::ConstantInt::get(Val->getType(), 0));
    return Builder.CreateNot(Val);
  }

  return nullptr;
}
