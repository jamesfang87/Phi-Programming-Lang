#include "CodeGen/CodeGen.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Control Flow Utilities
//===----------------------------------------------------------------------===//

void CodeGen::breakIntoBB(llvm::BasicBlock *Target) {
  llvm::BasicBlock *Current = Builder.GetInsertBlock();

  if (Current && !Current->getTerminator())
    Builder.CreateBr(Target);

  Builder.SetInsertPoint(Target);
}

void CodeGen::generateTerminatorIfNeeded(llvm::BasicBlock *Target) {
  llvm::BasicBlock *Current = Builder.GetInsertBlock();
  if (Current && !Current->getTerminator()) {
    Builder.CreateBr(Target);
  }
}

bool CodeGen::hasTerminator() const {
  llvm::BasicBlock *Current = Builder.GetInsertBlock();
  return Current && Current->getTerminator();
}

} // namespace phi
