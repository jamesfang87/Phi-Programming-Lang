#include "CodeGen/CodeGen.hpp"
#include "AST/Decl.hpp"

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

  // sort so StructDecls come first

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
  system(std::format("clang {}", IRFileName).c_str());
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

llvm::Value *CodeGen::store(llvm::Value *Val, llvm::Value *Destination,
                            const Type &T) {
  if (!T.isCustom())
    return Builder.CreateStore(Val, Destination);

  auto &DataLayout = Module.getDataLayout();
  auto *StructLayout = DataLayout.getStructLayout(
      static_cast<llvm::StructType *>(T.toLLVM(Context)));

  return Builder.CreateMemCpy(Destination, StructLayout->getAlignment(), Val,
                              StructLayout->getAlignment(),
                              StructLayout->getSizeInBytes());
}

llvm::Value *CodeGen::load(llvm::Value *Val, const Type &T) {
  return Builder.CreateLoad(T.toLLVM(Context), Val);
}

void CodeGen::generateMainWrapper() {
  auto *BuiltinMain = Module.getFunction("main");
  BuiltinMain->setName("__builtin_main");

  auto *Main = llvm::Function::Create(
      llvm::FunctionType::get(Builder.getInt32Ty(), {}, false),
      llvm::Function::ExternalLinkage, "main", Module);

  auto *Entry = llvm::BasicBlock::Create(Context, "entry", Main);
  Builder.SetInsertPoint(Entry);

  Builder.CreateCall(BuiltinMain);
  Builder.CreateRet(llvm::ConstantInt::getSigned(Builder.getInt32Ty(), 0));
}
