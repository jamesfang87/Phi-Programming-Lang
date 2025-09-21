#include "CodeGen/CodeGen.hpp"

using namespace phi;

llvm::Value *CodeGen::visit(FieldAccessExpr &E) {
  llvm::Value *Base = visit(*E.getBase());
  llvm::Value *Field = Builder.CreateStructGEP(
      E.getBase()->getType().toLLVM(Context), Base, E.getField()->getIndex());
  return Field;
}

llvm::Value *CodeGen::visit(MethodCallExpr &E) {}
