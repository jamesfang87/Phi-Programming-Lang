#include "CodeGen/CodeGen.hpp"

#include <map>

#include <llvm/ADT/APFloat.h>
#include <llvm/ADT/APInt.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>
#include <llvm/Support/Casting.h>

#include "AST/Type.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(Expr &E) { return E.accept(*this); }

llvm::Value *CodeGen::visit(IntLiteral &E) {
  return llvm::ConstantInt::get(E.getType().toLLVM(Context), E.getValue());
}

llvm::Value *CodeGen::visit(FloatLiteral &E) {
  return llvm::ConstantFP::get(E.getType().toLLVM(Context), E.getValue());
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
  llvm::Value *StartVal = visit(E.getStart());
  llvm::Value *EndVal = visit(E.getEnd());

  // store start to a temporary alloca in case it's referenced elsewhere by
  // lowered code
  llvm::AllocaInst *Temp =
      Builder.CreateAlloca(StartVal->getType(), nullptr, "range.start.tmp");
  Builder.CreateStore(StartVal, Temp);
  (void)Temp;
  return EndVal;
}

llvm::Value *CodeGen::visit(StructLiteral &E) {
  phi::Type Type = E.getType();
  llvm::Value *Temp = stackAlloca(*Type.getCustomName() + ".tmp", Type);

  std::map<const FieldDecl *, llvm::Value *> Inits;
  for (auto &&Init : E.getFields()) {
    llvm::Value *Val = visit(*Init->getValue());
    Inits[Init->getDecl()] = Val;
  }

  size_t I = 0;
  for (auto &&Field : E.getStructDecl()->getFields()) {
    llvm::Value *Dst = Builder.CreateStructGEP(Type.toLLVM(Context), Temp, I++);
    store(Inits[Field.get()], Dst, Field->getType());
  }

  return Temp;
}
