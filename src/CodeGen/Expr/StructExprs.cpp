#include "CodeGen/CodeGen.hpp"
#include "llvm/Support/Casting.h"
#include <cassert>

using namespace phi;

llvm::Value *CodeGen::visit(FieldAccessExpr &E) {
  llvm::Value *Base = visit(*E.getBase());
  llvm::Value *Field = Builder.CreateStructGEP(
      E.getBase()->getType().toLLVM(Context), Base, E.getField()->getIndex());
  return Field;
}

llvm::Value *CodeGen::visit(MethodCallExpr &E) {
  //   // Desugar method call into a regular function call
  //   // Method name is mangled as: StructName.methodName
  //
  //   // Get the base object (the struct instance)
  //   llvm::Value *BaseVal = visit(*E.getBase());
  //   Type BaseType = E.getBase()->getType();
  //
  //   // Create the mangled function name: StructName.methodName
  //   std::string MangledName =
  //       *BaseType.getStructName() + "." +
  //       llvm::dyn_cast<DeclRefExpr>(&E.getCallee())->getId();
  //
  //   // Look up the function in the module
  //   llvm::Function *Fun = Module.getFunction(MangledName);
  //   assert(Fun);
  //
  //   // Prepare arguments: [baseValue, ...originalArgs]
  //   std::vector<llvm::Value *> Args = {BaseVal};
  //   for (auto &Arg : E.getArgs()) {
  //     llvm::Value *ArgVal = load(visit(*Arg), Arg->getType());
  //     Args.push_back(ArgVal);
  //   }
  //
  //   // Create the function call
  //   llvm::Value *CallResult = Builder.CreateCall(Fun, Args);
  //   return Fun->getReturnType()->isVoidTy() ? nullptr : CallResult;
}
