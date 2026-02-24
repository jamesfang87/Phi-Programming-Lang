#include "CodeGen/LLVMCodeGen.hpp"

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/IR/Constants.h>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/Verifier.h>
#include <llvm/Support/raw_ostream.h>

using namespace phi;

void CodeGen::codegenModule(ModuleDecl *M) {
  // Pass 1: Declare struct types
  declareStructTypes(M);

  // Pass 2: Declare enum types
  declareEnumTypes(M);

  // Pass 3: Declare function signatures
  declareFunctions(M);

  // Pass 4: Generate function bodies
  generateFunctionBodies(M);
}

void CodeGen::declareStructTypes(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        getOrCreateStructType(S);
      }
    }
  }
}

void CodeGen::declareEnumTypes(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        getOrCreateEnumType(E);
      }
    }
  }
}

void CodeGen::declareFunctions(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      if (!F->hasTypeArgs()) {
        codegenFunctionDecl(F);
      }
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        for (auto &Method : S->getMethods()) {
          std::string MangledName = S->getId() + "_" + Method->getId();
          codegenMethodDecl(Method.get(), MangledName);
        }
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        for (auto &Method : E->getMethods()) {
          std::string MangledName = E->getId() + "_" + Method->getId();
          codegenMethodDecl(Method.get(), MangledName);
        }
      }
    }
  }
}

void CodeGen::generateFunctionBodies(ModuleDecl *M) {
  for (auto &Item : M->getItems()) {
    if (auto *F = llvm::dyn_cast<FunDecl>(Item.get())) {
      if (!F->hasTypeArgs()) {
        auto It = Functions.find(F);
        if (It != Functions.end()) {
          codegenFunctionBody(F, It->second);
        }
      }
    } else if (auto *S = llvm::dyn_cast<StructDecl>(Item.get())) {
      if (!S->hasTypeArgs()) {
        for (auto &Method : S->getMethods()) {
          auto It = Methods.find(Method.get());
          if (It != Methods.end()) {
            codegenMethodBody(Method.get(), It->second);
          }
        }
      }
    } else if (auto *E = llvm::dyn_cast<EnumDecl>(Item.get())) {
      if (!E->hasTypeArgs()) {
        for (auto &Method : E->getMethods()) {
          auto It = Methods.find(Method.get());
          if (It != Methods.end()) {
            codegenMethodBody(Method.get(), It->second);
          }
        }
      }
    }
  }
}

llvm::Function *CodeGen::codegenFunctionDecl(FunDecl *F) {
  // Build parameter types
  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : F->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  // Get return type
  llvm::Type *RetTy = getLLVMType(F->getReturnType());

  // Create function type
  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);

  // Create function
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    F->getId(), Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(F->getParams()[Idx++]->getId());
  }

  Functions[F] = Fn;
  return Fn;
}

llvm::Function *CodeGen::codegenMethodDecl(MethodDecl *M,
                                           const std::string &MangledName) {
  // Build parameter types (including 'this' as first param)
  std::vector<llvm::Type *> ParamTypes;
  for (const auto &P : M->getParams()) {
    ParamTypes.push_back(getLLVMType(P->getType()));
  }

  // Get return type
  llvm::Type *RetTy = getLLVMType(M->getReturnType());

  // Create function type
  auto *FnTy = llvm::FunctionType::get(RetTy, ParamTypes, false);

  // Create function
  auto *Fn = llvm::Function::Create(FnTy, llvm::Function::ExternalLinkage,
                                    MangledName, Module);

  // Name parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    Arg.setName(M->getParams()[Idx++]->getId());
  }

  Methods[M] = Fn;
  return Fn;
}

void CodeGen::codegenFunctionBody(FunDecl *F, llvm::Function *Fn) {
  CurrentFunction = Fn;

  // Create entry block
  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fn);
  Builder.SetInsertPoint(EntryBB);

  // Handle println intrinsic
  if (F->getId().find("println") == 0 && Fn->arg_size() == 1) {
    // Get printf
    llvm::FunctionType *PrintfTy = llvm::FunctionType::get(
        Builder.getInt32Ty(), {Builder.getPtrTy()}, true);
    llvm::FunctionCallee Printf =
        Module.getOrInsertFunction("printf", PrintfTy);

    // Get argument
    llvm::Argument *Arg = Fn->getArg(0);
    llvm::Type *ArgTy = Arg->getType();

    // Determine format string
    std::string Fmt;
    if (ArgTy->isIntegerTy(32)) {
      Fmt = "%d\n";
    } else if (ArgTy->isDoubleTy()) {
      Fmt = "%f\n";
    } else if (ArgTy->isIntegerTy(1)) {
      Fmt = "%d\n";
    } else {
      Fmt = "Unknown type\n";
    }

    // Create global string
    llvm::Value *FmtStr = Builder.CreateGlobalStringPtr(Fmt, "fmt");

    // Call printf
    Builder.CreateCall(Printf, {FmtStr, Arg});
    Builder.CreateRetVoid();
    CurrentFunction = nullptr;
    return;
  }

  // Clear named values for this function
  NamedValues.clear();

  // Allocate and store parameters
  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    auto *P = F->getParams()[Idx++].get();
    llvm::Type *Ty = Arg.getType();
    auto *Alloca = createEntryBlockAlloca(Fn, P->getId(), Ty);
    Builder.CreateStore(&Arg, Alloca);
    NamedValues[P] = Alloca;
  }

  // Generate body
  codegenBlock(&F->getBody());

  // Add default return if no terminator
  if (!hasTerminator()) {
    llvm::Type *RetTy = Fn->getReturnType();
    if (RetTy->isVoidTy()) {
      Builder.CreateRetVoid();
    } else {
      Builder.CreateRet(llvm::Constant::getNullValue(RetTy));
    }
  }

  CurrentFunction = nullptr;
}

void CodeGen::codegenMethodBody(MethodDecl *M, llvm::Function *Fn) {
  CurrentFunction = Fn;

  auto *EntryBB = llvm::BasicBlock::Create(Context, "entry", Fn);
  Builder.SetInsertPoint(EntryBB);

  NamedValues.clear();

  unsigned Idx = 0;
  for (auto &Arg : Fn->args()) {
    auto *P = M->getParams()[Idx++].get();
    llvm::Type *Ty = Arg.getType();
    auto *Alloca = createEntryBlockAlloca(Fn, P->getId(), Ty);
    Builder.CreateStore(&Arg, Alloca);
    NamedValues[P] = Alloca;
  }

  codegenBlock(&M->getBody());

  if (!hasTerminator()) {
    llvm::Type *RetTy = Fn->getReturnType();
    if (RetTy->isVoidTy()) {
      Builder.CreateRetVoid();
    } else {
      Builder.CreateRet(llvm::Constant::getNullValue(RetTy));
    }
  }

  CurrentFunction = nullptr;
}
