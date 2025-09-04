#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

using namespace phi;

llvm::Value *CodeGen::visit(MemberAccessExpr &E) {
  llvm::Value *BasePtr = getAddressOf(E.getBase());
  const FieldDecl &FD = E.getField();
  llvm::Value *Ptr = computeMemberPointer(BasePtr, &FD);
  llvm::Type *FieldType = FD.getType().toLLVM(Context);
  return Builder.CreateLoad(FieldType, Ptr);
}

llvm::Value *CodeGen::visit(MemberFunCallExpr &E) {
  llvm::Value *Base = E.getBase()->accept(*this);
  // prepare args: self first
  std::vector<llvm::Value *> Args;
  Args.push_back(Base);
  for (auto &A : E.getCall().getArgs())
    Args.push_back(A->accept(*this));

  auto *CalleeRef = llvm::dyn_cast<DeclRefExpr>(&E.getCall().getCallee());
  if (!CalleeRef) {
    throw std::runtime_error("unsupported callee type");
  }
  std::string MethodName = CalleeRef->getId();

  // Find the struct that contains this method
  const MethodDecl &Method = E.getMethod();
  StructDecl *Parent = nullptr;

  // Search through all structs to find the one containing this method
  for (auto &D : AstList) {
    if (auto *SD = llvm::dyn_cast<StructDecl>(D.get())) {
      for (auto &M : SD->getMethods()) {
        if (&M == &Method) {
          Parent = SD;
          break;
        }
      }
      if (Parent)
        break;
    }
  }

  if (!Parent) {
    throw std::runtime_error("could not find parent struct for method: " +
                             MethodName);
  }

  // Use struct-prefixed method name (dot)
  std::string PrefixedName = Parent->getId() + "." + MethodName;
  llvm::Function *Fn = Module.getFunction(PrefixedName);

  if (!Fn) {
    throw std::runtime_error("member function not found: " + PrefixedName);
  }

  return Builder.CreateCall(Fn, Args);
}
