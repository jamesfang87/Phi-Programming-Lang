#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/IR/BasicBlock.h>
#include <llvm/Support/Casting.h>

namespace phi {

//===----------------------------------------------------------------------===//
// Break and Continue Stmts
//===----------------------------------------------------------------------===//

void CodeGen::visit(BreakStmt &S) {
  (void)S;
  llvm::BasicBlock *BreakTarget = getCurrentBreakTarget();
  if (!BreakTarget) {
    throw std::runtime_error("'break' used outside of a loop");
  }
  Builder.CreateBr(BreakTarget);
  // Note: Control flow ends here, so no need to set insert point
}

void CodeGen::visit(ContinueStmt &S) {
  (void)S;
  llvm::BasicBlock *ContinueTarget = getCurrentContinueTarget();
  if (!ContinueTarget) {
    throw std::runtime_error("'continue' used outside of a loop");
  }
  Builder.CreateBr(ContinueTarget);
  // Note: Control flow ends here, so no need to set insert point
}

//===----------------------------------------------------------------------===//
// Loop Context Management
//===----------------------------------------------------------------------===//

void CodeGen::pushLoopContext(llvm::BasicBlock *BreakBB,
                              llvm::BasicBlock *ContinueBB) {
  LoopStack.emplace_back(BreakBB, ContinueBB);
}

void CodeGen::popLoopContext() {
  if (!LoopStack.empty()) {
    LoopStack.pop_back();
  }
}

llvm::BasicBlock *CodeGen::getCurrentBreakTarget() {
  if (LoopStack.empty()) {
    return nullptr;
  }
  return LoopStack.back().BreakTarget;
}

llvm::BasicBlock *CodeGen::getCurrentContinueTarget() {
  if (LoopStack.empty()) {
    return nullptr;
  }
  return LoopStack.back().ContinueTarget;
}

} // namespace phi
