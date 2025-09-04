#include "CodeGen/CodeGen.hpp"

using namespace phi;

llvm::Value *CodeGen::generatePrintlnBridge(FunCallExpr &Call) {
  ensurePrintfDeclared();

  std::vector<llvm::Value *> Args;
  if (Call.getArgs().empty()) {
    llvm::Value *Fmt = Builder.CreateGlobalString("\n");
    Args.push_back(Fmt);
  } else if (Call.getArgs().size() == 1) {
    llvm::Value *V = Call.getArgs()[0]->accept(*this);
    llvm::Value *Fmt;
    if (Call.getArgs()[0]->getType().isInteger()) {
      Fmt = Builder.CreateGlobalString("%lld\n");
      if (V->getType()->isIntegerTy() &&
          V->getType()->getIntegerBitWidth() != 64)
        V = Builder.CreateSExt(V, Builder.getInt64Ty());
    } else if (Call.getArgs()[0]->getType().isFloat()) {
      Fmt = Builder.CreateGlobalString("%g\n");
      if (!V->getType()->isDoubleTy())
        V = Builder.CreateFPExt(V, Builder.getDoubleTy());
    } else {
      Fmt = Builder.CreateGlobalString("%s\n");
    }
    Args.push_back(Fmt);
    Args.push_back(V);
  } else {
    // fallback: treat first as string format
    llvm::Value *FmtExpr = Call.getArgs()[0]->accept(*this);
    (void)FmtExpr; // we still use constant format to be safe
    llvm::Value *Fmt = Builder.CreateGlobalString("%s\n");
    Args.push_back(Fmt);
    Args.push_back(Call.getArgs()[0]->accept(*this));
    for (size_t I = 1; I < Call.getArgs().size(); ++I)
      Args.push_back(Call.getArgs()[I]->accept(*this));
  }

  return Builder.CreateCall(PrintfFn, Args);
}
