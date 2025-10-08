#include "CodeGen/CodeGen.hpp"
#include <cassert>
#include <print>
#include <string>

using namespace phi;

llvm::Value *CodeGen::visit(FieldAccessExpr &E) {
  auto T = E.getBase()->getType();
  assert(T.isCustom() || T.isPtr() || T.isRef());

  llvm::Type *LlvmType = T.toLLVM(Context);
  if (T.isPtr()) {
    LlvmType = T.asPtr().Pointee->toLLVM(Context);
  } else if (T.isRef()) {
    LlvmType = T.asRef().Pointee->toLLVM(Context);
  }

  llvm::Value *Base = visit(*E.getBase());
  llvm::Value *Field =
      Builder.CreateStructGEP(LlvmType, Base, E.getField()->getIndex());
  return Field;
}

llvm::Value *CodeGen::visit(MethodCallExpr &E) {
  // Desugar method call into a regular function call
  // Method name is mangled as: StructName.methodName

  // Get the base object (the struct instance)
  llvm::Value *BaseVal = visit(*E.getBase());
  Type BaseType = E.getBase()->getType();

  std::string StructName;
  if (BaseType.isRef()) {
    StructName = *BaseType.asRef().Pointee->getCustomName();
  } else if (BaseType.isPtr()) {
    StructName = *BaseType.asPtr().Pointee->getCustomName();
  } else if (BaseType.isCustom()) {
    StructName = *BaseType.getCustomName();
  }

  // Create the mangled function name: StructName.methodName
  std::string MangledName =
      StructName + "." + llvm::dyn_cast<DeclRefExpr>(&E.getCallee())->getId();

  // Look up the function in the module
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
      if (E.getBase()->getType().isCustom()) {
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
      if (Arg->getType().isCustom())
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
