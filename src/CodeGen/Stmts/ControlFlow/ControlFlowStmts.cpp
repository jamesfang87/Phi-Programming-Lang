#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/IR/BasicBlock.h>
#include <llvm/Support/Casting.h>

namespace phi {

//===----------------------------------------------------------------------===//
// For Loop Generation
//===----------------------------------------------------------------------===//

void CodeGen::visit(ForStmt &S) {
  assert(CurrentFun != nullptr);

  auto Blocks = createForLoopBlocks();
  auto RangeInfo = extractRangeInfo(S);
  pushLoopContext(Blocks.ExitBB, Blocks.IncBB);

  generateForInit(S, RangeInfo, Blocks);
  generateForCondition(S, RangeInfo, Blocks);
  generateForBody(S, Blocks);
  generateForIncrement(S, RangeInfo, Blocks);

  popLoopContext();
  Builder.SetInsertPoint(Blocks.ExitBB);
}

//===----------------------------------------------------------------------===//
// While Loop Generation
//===----------------------------------------------------------------------===//

void CodeGen::visit(WhileStmt &S) {
  assert(CurrentFun != nullptr);

  auto Blocks = createWhileLoopBlocks();
  pushLoopContext(Blocks.ExitBB, Blocks.CondBB);

  generateWhileCondition(S, Blocks);
  generateWhileBody(S, Blocks);

  popLoopContext();
  Builder.SetInsertPoint(Blocks.ExitBB);
}

//===----------------------------------------------------------------------===//
// If Statement Generation
//===----------------------------------------------------------------------===//

void CodeGen::visit(IfStmt &S) {
  assert(CurrentFun != nullptr);

  auto Blocks = createIfStatementBlocks(S);
  generateIfCondition(S, Blocks);
  generateIfBranches(S, Blocks);

  Builder.SetInsertPoint(Blocks.ExitBB);
}

} // namespace phi
