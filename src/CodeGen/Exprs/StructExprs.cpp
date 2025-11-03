#include "CodeGen/CodeGen.hpp"
#include <cassert>
#include <string>

using namespace phi;

llvm::Value *CodeGen::visit(FieldAccessExpr &E) {
  auto T = E.getBase()->getType().getUnderlying().toLLVM(Context);

  llvm::Value *Base = visit(*E.getBase());
  llvm::Value *Field =
      Builder.CreateStructGEP(T, Base, E.getField()->getIndex());
  return Field;
}

llvm::Value *CodeGen::visit(MethodCallExpr &E) {
  // Get the base object (the struct instance)
  llvm::Value *BaseVal = visit(*E.getBase());

  // Desugar method call into a regular function call
  // Method name is mangled as: StructName.methodName
  // And look up the function in the Module
  std::string Name = *E.getBase()->getType().getUnderlying().getCustomName();
  std::string MangledName = std::format(
      "{}.{}", Name, llvm::dyn_cast<DeclRefExpr>(&E.getCallee())->getId());
  llvm::Function *Fun = Module.getFunction(MangledName);
  assert(Fun && "Did not find function");

  // Prepare arguments with parameter-type aware handling
  std::vector<llvm::Value *> Args;
  Args.reserve(1 + E.getArgs().size());

  // Handle 'this' (first param)
  if (Fun->getFunctionType()->getNumParams() > 0) {
    llvm::Type *FirstParamTy = Fun->getFunctionType()->getParamType(0);
    if (FirstParamTy->isPointerTy()) {
      // pass pointer 'this' as-is
      Args.push_back(BaseVal);
    } else {
      // pass by-value: load the aggregate or primitive
      if (E.getBase()->getType().isStruct()) {
        Args.push_back(Builder.CreateLoad(
            E.getBase()->getType().toLLVM(Context), BaseVal));
      } else {
        Args.push_back(load(BaseVal, E.getBase()->getType()));
      }
    }
  } else {
    Args.push_back(BaseVal);
  }

  // Now handle other parameters
  unsigned ParamIndex = 1;
  for (auto &Arg : E.getArgs()) {
    llvm::Value *Raw = visit(*Arg);
    llvm::Type *ParamTy = nullptr;
    if (ParamIndex < Fun->getFunctionType()->getNumParams())
      ParamTy = Fun->getFunctionType()->getParamType(ParamIndex);

    llvm::Value *ArgToPass = nullptr;
    if (ParamTy && ParamTy->isPointerTy()) {
      ArgToPass = Raw;
    } else {
      if (Arg->getType().isStruct())
        ArgToPass = Builder.CreateLoad(Arg->getType().toLLVM(Context), Raw);
      else
        ArgToPass = load(Raw, Arg->getType());
    }

    Args.push_back(ArgToPass);
    ++ParamIndex;
  }

  llvm::Value *CallResult = Builder.CreateCall(Fun, Args);

  return Fun->getReturnType()->isVoidTy() ? nullptr : CallResult;
}
