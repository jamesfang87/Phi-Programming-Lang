#include "CodeGen/CodeGen.hpp"

#include <llvm/IR/Instructions.h> // LoadInst
#include <llvm/Support/Casting.h>

using namespace phi;

llvm::Value *CodeGen::visit(FieldAccessExpr &E) {
  // Get the address of the base object (must be an address)
  llvm::Value *BasePtr = getAddressOf(E.getBase());
  if (!BasePtr)
    throw std::runtime_error("failed to get address of member base");

  const FieldDecl &FD = *E.getField();

  // Compute pointer to the field (may return i8* or some other pointer)
  llvm::Value *Ptr = computeMemberPointer(BasePtr, &FD);
  if (!Ptr)
    throw std::runtime_error("failed to compute member pointer");

  // Get the LLVM type of the field (e.g., i32) and the pointer type we need
  // (i32*)
  llvm::Type *FieldType = FD.getType().toLLVM(Context);
  if (!FieldType)
    throw std::runtime_error("invalid field type");

  llvm::PointerType *ExpectedPtrTy =
      llvm::PointerType::get(Context, 0); // opaque pointer in addrspace(0)

  // Ensure the pointer has the expected pointer type (bitcast if necessary)
  if (Ptr->getType() != ExpectedPtrTy) {
    Ptr = Builder.CreateBitCast(Ptr, ExpectedPtrTy, "field.ptr.cast");
  }

  // Now load with the correct pointer type
  return Builder.CreateLoad(FieldType, Ptr, "field.load");
}

llvm::Value *CodeGen::visit(MethodCallExpr &E) {
  auto *CalleeRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
  if (!CalleeRef) {
    throw std::runtime_error("unsupported callee type");
  }

  // IMPORTANT: use the ADDRESS of the base (self), not the loaded value.
  // Passing loaded values for 'self' often leads to passing uninitialized or
  // invalid pointers.
  llvm::Value *BasePtr = getAddressOf(E.getBase());
  if (!BasePtr)
    throw std::runtime_error("failed to get address of method base");

  // prepare args: self first
  std::vector<llvm::Value *> Args;
  Args.push_back(BasePtr);

  for (auto &A : E.getArgs()) {
    llvm::Value *ArgV = A->accept(*this);
    if (!ArgV)
      throw std::runtime_error("failed to generate argument");
    Args.push_back(ArgV);
  }

  // Find the struct that contains this method
  const MethodDecl &Method = E.getMethod();
  const StructDecl *Parent = Method.getParent();

  // Use struct-prefixed method name (dot)
  std::string PrefixedName = Parent->getId() + "." + Method.getId();
  llvm::Function *Fun = Module.getFunction(PrefixedName);

  if (!Fun) {
    throw std::runtime_error("member function not found: " + PrefixedName);
  }

  // Make sure the 'self' (first arg) has the type the function expects.
  // If not, bitcast the pointer to the expected type.
  const llvm::FunctionType *FT = Fun->getFunctionType();
  if (FT->getNumParams() > 0) {
    llvm::Type *ExpectedFirstParamTy = FT->getParamType(0);
    if (Args[0]->getType() != ExpectedFirstParamTy) {
      // If our BasePtr is a pointer to struct but the function expects an i8*
      // (or vice-versa), perform a bitcast to the expected type.
      Args[0] =
          Builder.CreateBitCast(Args[0], ExpectedFirstParamTy, "self.cast");
    }
  }

  // If argument count / types mismatch, you may want to validate here and adapt
  // (not shown).

  return Builder.CreateCall(Fun, Args, "call.member");
}
