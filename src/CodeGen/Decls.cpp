#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::visit(FunDecl &F) {
  if (F.getId() == "println")
    return;

  // Build function type directly from the AST declaration (do not rely on
  // getFunType().toLLVM(), which may be unreliable for your AST).
  llvm::Type *RetTy = F.getReturnTy().toLLVM(Context);
  if (!RetTy)
    throw std::runtime_error("could not lower return type for function: " +
                             F.getId());

  std::vector<llvm::Type *> ParamTypes;
  for (auto &P : F.getParams()) {
    llvm::Type *PT = P->getType().toLLVM(Context);
    if (!PT)
      throw std::runtime_error("could not lower parameter type for function: " +
                               F.getId());
    ParamTypes.push_back(PT);
  }

  llvm::FunctionType *FTy =
      llvm::FunctionType::get(RetTy, ParamTypes, /*isVarArg=*/false);

  std::string FunctionName = F.getId();
  // NOTE: method decls are handled inside StructDecl generation; top-level
  // FunDecl here is a normal function.

  llvm::Function *Fn = llvm::Function::Create(
      FTy, llvm::Function::ExternalLinkage, FunctionName, &Module);

  // set current function for defer handling
  CurrentFunction = Fn;

  // entry block
  llvm::BasicBlock *Entry = llvm::BasicBlock::Create(Context, "entry", Fn);
  Builder.SetInsertPoint(Entry);

  // allocate and store parameters
  auto ArgIt = Fn->arg_begin();
  for (auto &P : F.getParams()) {
    if (ArgIt == Fn->arg_end())
      throw std::runtime_error("argument iterator exhausted for function: " +
                               F.getId());

    llvm::Type *PT = P->getType().toLLVM(Context);
    llvm::AllocaInst *A = Builder.CreateAlloca(PT, nullptr, P->getId());
    Builder.CreateStore(&(*ArgIt), A);
    DeclMap[P.get()] = A;
    ++ArgIt;
  }

  // emit body statements
  for (auto &StmtPtr : F.getBody().getStmts()) {
    StmtPtr->accept(*this); // statements return void
  }

  // cleanup block
  llvm::BasicBlock *Cleanup = nullptr;
  auto CI = CleanupBlockMap.find(Fn);
  if (CI == CleanupBlockMap.end()) {
    Cleanup = llvm::BasicBlock::Create(Context, "func.cleanup", Fn);
    CleanupBlockMap[Fn] = Cleanup;
  } else
    Cleanup = CI->second;

  if (!Builder.GetInsertBlock()->getTerminator())
    Builder.CreateBr(Cleanup);

  Builder.SetInsertPoint(Cleanup);
  emitDeferredForFunction(Fn);

  auto RAIt = ReturnAllocaMap.find(Fn);
  if (RAIt != ReturnAllocaMap.end()) {
    llvm::AllocaInst *RA = RAIt->second;
    llvm::Value *Rv =
        Builder.CreateLoad(RA->getAllocatedType(), RA, "ret.load");
    Builder.CreateRet(Rv);
  } else {
    if (Fn->getReturnType()->isVoidTy())
      Builder.CreateRetVoid();
    else
      Builder.CreateRet(llvm::Constant::getNullValue(Fn->getReturnType()));
  }

  CurrentFunction = nullptr;
}

void CodeGen::visit(ParamDecl &D) { (void)D; }

void CodeGen::visit(StructDecl &D) {
  createStructLayout(&D);

  // Generate methods with struct prefix (includes implicit self)
  for (auto &Method : D.getMethods()) {
    if (Method.getId() == "println")
      continue;

    // Build the method's return and parameter types from the AST (AST params
    // do NOT include 'self')
    llvm::Type *RetTy = Method.getReturnTy().toLLVM(Context);
    if (!RetTy)
      throw std::runtime_error("could not lower return type for method: " +
                               Method.getId());

    std::vector<llvm::Type *> ParamTypes;
    for (auto &P : Method.getParams()) {
      llvm::Type *PT = P->getType().toLLVM(Context);
      if (!PT)
        throw std::runtime_error("could not lower parameter type for method: " +
                                 Method.getId());
      ParamTypes.push_back(PT);
    }

    // Prepend implicit self: pointer to the struct type
    llvm::StructType *ST = StructTypeMap.at(&D);
    if (!ST)
      throw std::runtime_error("struct type missing when generating method: " +
                               D.getId());
    std::vector<llvm::Type *> MethodParams;
    MethodParams.push_back(llvm::PointerType::getUnqual(ST)); // self*
    MethodParams.insert(MethodParams.end(), ParamTypes.begin(),
                        ParamTypes.end());

    llvm::FunctionType *FTy =
        llvm::FunctionType::get(RetTy, MethodParams, /*isVarArg=*/false);

    // Use dot for desugared method names: Struct.Method
    std::string MethodName = D.getId() + "." + Method.getId();
    llvm::Function *Fn = llvm::Function::Create(
        FTy, llvm::Function::ExternalLinkage, MethodName, &Module);

    // set current function for defer handling
    CurrentFunction = Fn;

    // entry block
    llvm::BasicBlock *Entry = llvm::BasicBlock::Create(Context, "entry", Fn);
    Builder.SetInsertPoint(Entry);

    // Map 'self' and map fields -> self pointer so field DeclRefExpr lookups
    // work.
    auto ArgIt = Fn->arg_begin();
    if (ArgIt == Fn->arg_end())
      throw std::runtime_error("method has no arguments (expected self) for: " +
                               MethodName);
    llvm::Value *SelfArg = &(*ArgIt); // the function's first argument: self*

    // Temporarily inject FieldDecl -> self mapping so DeclRefExpr(FieldDecl)
    // can find self pointer and compute member pointers.
    std::vector<const FieldDecl *> InjectedFieldKeys;
    for (auto &Fdecl : D.getFields()) {
      DeclMap[Fdecl.get()] = SelfArg;
      InjectedFieldKeys.push_back(Fdecl.get());
    }

    // Skip the implicit self arg before mapping AST-visible params
    ++ArgIt;

    // allocate and store AST-visible parameters
    for (auto &P : Method.getParams()) {
      if (ArgIt == Fn->arg_end())
        throw std::runtime_error("argument iterator exhausted for method: " +
                                 MethodName);

      llvm::Type *PT = P->getType().toLLVM(Context);
      llvm::AllocaInst *A = Builder.CreateAlloca(PT, nullptr, P->getId());
      Builder.CreateStore(&(*ArgIt), A);
      DeclMap[P.get()] = A;
      ++ArgIt;
    }

    // emit body statements
    for (auto &StmtPtr : Method.getBody().getStmts()) {
      StmtPtr->accept(*this); // statements return void
    }

    // cleanup block
    llvm::BasicBlock *Cleanup = nullptr;
    auto CI = CleanupBlockMap.find(Fn);
    if (CI == CleanupBlockMap.end()) {
      Cleanup = llvm::BasicBlock::Create(Context, "func.cleanup", Fn);
      CleanupBlockMap[Fn] = Cleanup;
    } else
      Cleanup = CI->second;

    if (!Builder.GetInsertBlock()->getTerminator())
      Builder.CreateBr(Cleanup);

    Builder.SetInsertPoint(Cleanup);
    emitDeferredForFunction(Fn);

    auto RAIt = ReturnAllocaMap.find(Fn);
    if (RAIt != ReturnAllocaMap.end()) {
      llvm::AllocaInst *RA = RAIt->second;
      llvm::Value *Rv =
          Builder.CreateLoad(RA->getAllocatedType(), RA, "ret.load");
      Builder.CreateRet(Rv);
    } else {
      if (Fn->getReturnType()->isVoidTy())
        Builder.CreateRetVoid();
      else
        Builder.CreateRet(llvm::Constant::getNullValue(Fn->getReturnType()));
    }

    // remove injected field -> self mappings so other functions are not
    // affected
    for (auto *K : InjectedFieldKeys)
      DeclMap.erase(const_cast<FieldDecl *>(K));

    CurrentFunction = nullptr;
  }
}

void CodeGen::visit(FieldDecl &D) { (void)D; }
void CodeGen::visit(VarDecl &D) { (void)D; }
