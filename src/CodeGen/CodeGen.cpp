#include "CodeGen/CodeGen.hpp"
#include "AST/Decl.hpp"

#include <cassert>
#include <stdexcept>
#include <string_view>
#include <system_error>

#include <llvm/Support/Casting.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/TargetParser/Triple.h>

using namespace phi;

CodeGen::CodeGen(std::vector<std::unique_ptr<Decl>> Ast,
                 std::string_view SourcePath)
    : Path(SourcePath), Ast(std::move(Ast)), Context(), Builder(Context),
      Module(std::string(SourcePath), Context) {
  Module.setSourceFileName(std::string(SourcePath));
  Module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
}

void CodeGen::generate() {
  declarePrint();

  for (auto &D : Ast) {
    if (auto Struct = llvm::dyn_cast<StructDecl>(D.get())) {
      declareHeader(*Struct);
    }

    if (auto Fun = llvm::dyn_cast<FunDecl>(D.get())) {
      declareHeader(*Fun);
    }
  }

  for (auto &D : Ast) {
    visit(*D);
  }

  generateMainWrapper();

  // Output IR to file
  std::string IRFileName = Path;
  size_t DotPos = IRFileName.find_last_of('.');
  if (DotPos != std::string::npos) {
    IRFileName = IRFileName.substr(0, DotPos);
  }
  IRFileName += ".ll";

  outputIR(IRFileName);
  system(std::format("clang -o ~/Phi/a.out {}", IRFileName).c_str());
  system(std::format("rm {}", IRFileName).c_str());
}

void CodeGen::outputIR(const std::string &Filename) {
  std::error_code EC;
  llvm::raw_fd_ostream File(Filename, EC);
  if (EC)
    throw std::runtime_error("Could not open file: " + EC.message());
  Module.print(File, nullptr);
}

llvm::AllocaInst *CodeGen::stackAlloca(Decl &D) {
  std::string_view Id = D.getId();
  Type T = D.getType();

  llvm::IRBuilder<> TempBuilder(Context);
  TempBuilder.SetInsertPoint(AllocaInsertPoint);

  return TempBuilder.CreateAlloca(T.toLLVM(Context), nullptr, Id);
}

llvm::AllocaInst *CodeGen::stackAlloca(std::string_view Id, const Type &T) {
  llvm::IRBuilder<> TempBuilder(Context);
  TempBuilder.SetInsertPoint(AllocaInsertPoint);

  return TempBuilder.CreateAlloca(T.toLLVM(Context), nullptr, Id);
}

llvm::Value *CodeGen::store(llvm::Value *Val, llvm::Value *Destination,
                            const Type &T) {
  if (!T.isStruct())
    return Builder.CreateStore(Val, Destination);

  // T.toLLVM(Context) should return the llvm::StructType* for the struct
  auto *StructTy = static_cast<llvm::StructType *>(T.toLLVM(Context));
  unsigned NumFields = StructTy->getNumElements();

  // We expect Val and Destination to be pointers to the struct (allocas).
  // For each field: load from src.field and store into dst.field.
  for (unsigned i = 0; i < NumFields; ++i) {
    // GEP to the i-th element for src and dst
    llvm::Value *DstGEP = Builder.CreateStructGEP(StructTy, Destination, i);
    llvm::Value *SrcGEP = Builder.CreateStructGEP(StructTy, Val, i);

    // Load the element from source and store into destination
    llvm::Type *ElemTy = StructTy->getElementType(i);
    llvm::Value *Loaded = Builder.CreateLoad(ElemTy, SrcGEP);
    Builder.CreateStore(Loaded, DstGEP);
  }

  // Return the destination pointer (consistent with your previous API)
  return Destination;
}

llvm::Value *CodeGen::load(llvm::Value *Val, const Type &T) {
  // If it's already a value (not an alloca), return it directly
  if (!llvm::isa<llvm::AllocaInst>(Val) &&
      !llvm::isa<llvm::GetElementPtrInst>(Val)) {
    return Val;
  }

  // For custom types (structs), we want the pointer, don't load the entire
  // struct
  if (T.isStruct()) {
    return Val;
  }

  // For primitive types, perform the actual load
  return Builder.CreateLoad(T.toLLVM(Context), Val);
}

void CodeGen::generateMainWrapper() {
  auto *BuiltinMain = Module.getFunction("main");
  assert(BuiltinMain);
  BuiltinMain->setName("__builtin_main");

  auto *Main = llvm::Function::Create(
      llvm::FunctionType::get(Builder.getInt32Ty(), {}, false),
      llvm::Function::ExternalLinkage, "main", Module);

  auto *Entry = llvm::BasicBlock::Create(Context, "entry", Main);
  Builder.SetInsertPoint(Entry);

  Builder.CreateCall(BuiltinMain);
  Builder.CreateRet(llvm::ConstantInt::getSigned(Builder.getInt32Ty(), 0));
}
