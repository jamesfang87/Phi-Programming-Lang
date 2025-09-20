#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"

#include <cassert>

using namespace phi;

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

//===----------------------------------------------------------------------===//
// Defer Statement Management
//===----------------------------------------------------------------------===//

void CodeGen::pushDefer(Expr &DeferredExpr) {
  DeferStack.emplace_back(DeferredExpr);
}

void CodeGen::executeDefers() {
  // Execute deferred statements in reverse order (LIFO)
  for (auto It = DeferStack.rbegin(); It != DeferStack.rend(); ++It) {
    visit(It->get());
  }
}

void CodeGen::clearDefers() { DeferStack.clear(); }

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
