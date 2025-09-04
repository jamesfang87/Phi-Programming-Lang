#include "CodeGen/CodeGen.hpp"

#include <llvm/ADT/APFloat.h>
#include <llvm/ADT/APInt.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>
#include <llvm/Support/Casting.h>

using namespace phi;

llvm::Value *CodeGen::visit(IntLiteral &E) {
  // Prefer the type from the AST for this literal.
  llvm::Type *Ty = E.getType().toLLVM(Context);
  if (!Ty) {
    // Defensive fallback to previous behavior
    return llvm::ConstantInt::get(Builder.getInt64Ty(), E.getValue());
  }

  if (auto *IT = llvm::dyn_cast<llvm::IntegerType>(Ty)) {
    unsigned Bits = IT->getBitWidth();

    // Determine signedness from AST Type. We expect Type has helpers like:
    // isUnsignedInteger() / isSignedInteger(). If not, adapt to whichever API
    // your Type class provides.
    bool isUnsigned = E.getType().isUnsignedInteger();

    // E.getValue() returns an integral value (likely 64-bit signed). We must
    // construct an APInt of the specified width. Use the signed-constructor
    // if the AST type is signed so negative literals are encoded correctly.
    auto Raw = static_cast<uint64_t>(E.getValue());
    llvm::APInt Api = isUnsigned ? llvm::APInt(Bits, Raw)
                                 : llvm::APInt(Bits, Raw, /*isSigned=*/true);

    return llvm::ConstantInt::get(Context, Api);
  }

  // If the declared type is not an integer, fallback to i64 constant so we
  // still return something usable (but this likely indicates a bug upstream).
  return llvm::ConstantInt::get(Builder.getInt64Ty(), E.getValue());
}

llvm::Value *CodeGen::visit(FloatLiteral &E) {
  llvm::Type *Ty = E.getType().toLLVM(Context);
  if (!Ty)
    return llvm::ConstantFP::get(Builder.getDoubleTy(), E.getValue());

  return llvm::ConstantFP::get(Ty, E.getValue());
}

llvm::Value *CodeGen::visit(StrLiteral &E) {
  return Builder.CreateGlobalString(E.getValue());
}

llvm::Value *CodeGen::visit(CharLiteral &E) {
  return llvm::ConstantInt::get(Builder.getInt8Ty(), E.getValue());
}

llvm::Value *CodeGen::visit(BoolLiteral &E) {
  return llvm::ConstantInt::get(Builder.getInt1Ty(), E.getValue() ? 1 : 0);
}

llvm::Value *CodeGen::visit(RangeLiteral &E) {
  // evaluate start and end; return end (previous code relied on end)
  llvm::Value *StartVal = E.getStart().accept(*this);
  llvm::Value *EndVal = E.getEnd().accept(*this);

  // store start to a temporary alloca in case it's referenced elsewhere by
  // lowered code
  llvm::AllocaInst *Tmp =
      Builder.CreateAlloca(StartVal->getType(), nullptr, "range.start.tmp");
  Builder.CreateStore(StartVal, Tmp);
  (void)Tmp;
  return EndVal;
}
