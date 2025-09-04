#include "CodeGen/CodeGen.hpp"

#include <stdexcept>
#include <system_error>

#include <llvm/Support/Casting.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/TargetParser/Triple.h>

using namespace phi;

CodeGen::CodeGen(std::vector<std::unique_ptr<Decl>> Ast,
                 std::string_view SourcePath, std::string_view TargetTriple)
    : AstList(std::move(Ast)), Context(), Builder(Context),
      Module(std::string(SourcePath), Context) {
  Module.setSourceFileName(std::string(SourcePath));
  if (TargetTriple.empty())
    Module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
  else
    Module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
}

void CodeGen::ensurePrintfDeclared() {
  if (PrintfFn)
    return;
  llvm::Type *I8Ptr = llvm::PointerType::get(Builder.getInt8Ty(), 0);
  llvm::FunctionType *FT =
      llvm::FunctionType::get(Builder.getInt32Ty(), {I8Ptr}, true);
  PrintfFn = llvm::Function::Create(FT, llvm::Function::ExternalLinkage,
                                    "printf", &Module);
}

void CodeGen::generate() {
  // Precreate struct layouts so functions can reference them
  for (auto &D : AstList)
    if (auto *SD = llvm::dyn_cast<StructDecl>(D.get()))
      createStructLayout(SD);

  ensurePrintfDeclared();

  // Generate top-level: functions and globals
  for (auto &D : AstList) {
    if (!D)
      continue;
    if (auto *FD = llvm::dyn_cast<FunDecl>(D.get())) {
      if (FD->getId() == "println")
        continue;
      visit(*FD);
    } else if (auto *VD = llvm::dyn_cast<VarDecl>(D.get())) {
      llvm::Type *GTy = VD->getType().toLLVM(Context);
      auto *Gv = new llvm::GlobalVariable(Module, GTy, false,
                                          llvm::GlobalValue::ExternalLinkage,
                                          nullptr, VD->getId());
      if (VD->hasInit()) {
        llvm::Value *Init = evaluateExprUsingVisit(VD->getInit());
        if (Init && llvm::isa<llvm::Constant>(Init))
          Gv->setInitializer(llvm::cast<llvm::Constant>(Init));
      }
      DeclMap[VD] = Gv;
    } else if (auto *SD = llvm::dyn_cast<StructDecl>(D.get())) {
      visit(*SD);
    } else {
      // struct layouts already created above
    }
  }
}

void CodeGen::outputIR(const std::string &Filename) {
  std::error_code EC;
  llvm::raw_fd_ostream File(Filename, EC);
  if (EC)
    throw std::runtime_error("Could not open file: " + EC.message());
  Module.print(File, nullptr);
}

llvm::Value *CodeGen::getAllocaForDecl(Decl *D) {
  auto It = DeclMap.find(D);
  if (It == DeclMap.end())
    return nullptr;
  return It->second;
}

void CodeGen::createStructLayout(StructDecl *S) {
  if (StructTypeMap.count(S))
    return;
  std::vector<llvm::Type *> FieldTypes;
  unsigned Idx = 0;
  for (auto &F : S->getFields()) {
    FieldTypes.push_back(F->getType().toLLVM(Context));
    FieldIndexMap[F.get()] = Idx++;
  }
  llvm::StructType *ST =
      llvm::StructType::create(Context, FieldTypes, S->getId());
  StructTypeMap[S] = ST;
}

StructDecl *CodeGen::findContainingStruct(const FieldDecl *F) {
  for (auto &D : AstList)
    if (auto *SD = llvm::dyn_cast<StructDecl>(D.get()))
      for (auto &Fp : SD->getFields())
        if (Fp.get() == F)
          return SD;
  return nullptr;
}

llvm::Value *CodeGen::computeMemberPointer(llvm::Value *BasePtr,
                                           const FieldDecl *Field) {
  if (!BasePtr)
    throw std::runtime_error("member base is null");

  StructDecl *Parent = findContainingStruct(Field);
  if (!Parent)
    throw std::runtime_error("member parent not found");

  llvm::StructType *ST = StructTypeMap.at(Parent);
  unsigned Idx = FieldIndexMap.at(Field);

  if (!BasePtr->getType()->isPointerTy())
    throw std::runtime_error("member access requires pointer to struct");

  // Always cast to the correct struct pointer type under opaque pointers.
  llvm::PointerType *DesiredPtrTy = llvm::PointerType::getUnqual(ST);
  BasePtr = Builder.CreateBitCast(BasePtr, DesiredPtrTy,
                                  Field->getId() + ".self.cast");

  // Now BasePtr is %Person*, so CreateStructGEP works.
  return Builder.CreateStructGEP(ST, BasePtr, Idx, Field->getId());
}

llvm::Value *CodeGen::getAddressOf(Expr *E) {
  // For DeclRefExpr, return the alloca directly instead of loading from it
  if (auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(E)) {
    Decl *D = DeclRef->getDecl();
    auto It = DeclMap.find(D);
    if (It == DeclMap.end())
      return nullptr;
    llvm::Value *Val = It->second;
    if (auto *Alloca = llvm::dyn_cast<llvm::AllocaInst>(Val))
      return Alloca; // Return the alloca directly, don't load
    return Val;
  }

  // For member access, recursively get the address and compute member pointer
  if (auto *MemberAccess = llvm::dyn_cast<MemberAccessExpr>(E)) {
    llvm::Value *BasePtr = getAddressOf(MemberAccess->getBase());
    if (!BasePtr)
      return nullptr;
    return computeMemberPointer(BasePtr, &MemberAccess->getField());
  }

  // For other expressions, fall back to normal evaluation
  // This may not work for all cases, but handles the common ones
  return E->accept(*this);
}
