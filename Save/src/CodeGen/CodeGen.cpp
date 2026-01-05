#include "CodeGen/CodeGen.hpp"
#include "AST/Nodes/Stmt.hpp"

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
  // system(std::format("rm {}", IRFileName).c_str());
}

void CodeGen::outputIR(const std::string &Filename) {
  std::error_code EC;
  llvm::raw_fd_ostream File(Filename, EC);
  if (EC)
    throw std::runtime_error("Could not open file: " + EC.message());
  Module.print(File, nullptr);
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
