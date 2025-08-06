#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"
#include "llvm/IR/Function.h"
#include <cassert>
#include <print>

void phi::CodeGen::visit(phi::ReturnStmt &stmt) {
  if (stmt.hasExpr()) {
    stmt.getExpr().accept(*this);
    builder.CreateRet(curValue);
  } else {
    builder.CreateRetVoid();
  }
}

void phi::CodeGen::visit(phi::IfStmt &stmt) {
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Evaluate condition
  stmt.getCond().accept(*this);
  llvm::Value *cond = curValue;

  bool hasElse = stmt.hasElse();
  bool hasThenBody = !stmt.getThen().getStmts().empty();
  bool hasElseBody = hasElse && !stmt.getElse().getStmts().empty();

  llvm::BasicBlock *then_block = nullptr;
  llvm::BasicBlock *else_block = nullptr;
  llvm::BasicBlock *merge_block = nullptr;

  if (hasThenBody) {
    then_block = llvm::BasicBlock::Create(context, "if.then", function);
  }
  if (hasElseBody) {
    else_block = llvm::BasicBlock::Create(context, "if.else", function);
  }

  // Need merge block if both branches are non-terminal
  bool needMerge = hasThenBody || hasElseBody;
  if (needMerge) {
    merge_block = llvm::BasicBlock::Create(context, "if.end", function);
  }

  // Choose targets for condition
  if (hasThenBody && hasElseBody) {
    builder.CreateCondBr(cond, then_block, else_block);
  } else if (hasThenBody) {
    builder.CreateCondBr(cond, then_block, merge_block);
  } else if (hasElseBody) {
    builder.CreateCondBr(cond, merge_block, else_block);
  } else {
    // both branches are empty; conditionally jump to merge
    builder.CreateBr(merge_block);
  }

  // THEN block
  if (hasThenBody) {
    builder.SetInsertPoint(then_block);
    for (auto &s : stmt.getThen().getStmts()) {
      s->accept(*this);
    }
    if (!builder.GetInsertBlock()->getTerminator()) {
      builder.CreateBr(merge_block);
    }
  }

  // ELSE block
  if (hasElseBody) {
    builder.SetInsertPoint(else_block);
    for (auto &s : stmt.getElse().getStmts()) {
      s->accept(*this);
    }
    if (!builder.GetInsertBlock()->getTerminator()) {
      builder.CreateBr(merge_block);
    }
  }

  // Merge
  if (merge_block) {
    builder.SetInsertPoint(merge_block);
  }
}

void phi::CodeGen::visit(phi::WhileStmt &stmt) {
  // Get current function
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Create basic blocks
  auto CondBlock = llvm::BasicBlock::Create(context, "while.cond", function);
  auto BodyBlock = llvm::BasicBlock::Create(context, "while.body", function);
  auto ExitBlock = llvm::BasicBlock::Create(context, "while.exit", function);

  // Enter loop context
  LoopStack.push_back({ExitBlock, CondBlock});

  // Generate cond block
  builder.CreateBr(CondBlock);
  builder.SetInsertPoint(CondBlock);
  stmt.getCond().accept(*this);
  llvm::Value *cond = curValue;
  builder.CreateCondBr(cond, BodyBlock, ExitBlock);

  // Generate body block
  builder.SetInsertPoint(BodyBlock);
  for (auto &body_stmt : stmt.getBody().getStmts()) {
    body_stmt->accept(*this);
  }
  // Branch back to cond if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(CondBlock);
  }
  LoopStack.pop_back();

  // Continue with exit block
  builder.SetInsertPoint(ExitBlock);
}

void phi::CodeGen::visit(phi::ForStmt &stmt) {
  // Get current function
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Create basic blocks
  auto InitBlock = llvm::BasicBlock::Create(context, "for.init", function);
  auto CondBlock = llvm::BasicBlock::Create(context, "for.cond", function);
  auto BodyBlock = llvm::BasicBlock::Create(context, "for.body", function);
  auto IncBlock = llvm::BasicBlock::Create(context, "for.inc", function);
  auto ExitBlock = llvm::BasicBlock::Create(context, "for.exit", function);

  // Jump to initialization block
  builder.CreateBr(InitBlock);

  // Generate initialization block
  builder.SetInsertPoint(InitBlock);

  // Create allocation for loop variable
  VarDecl &loop_var = stmt.getLoopVar();
  llvm::Type *var_type = getType(loop_var.getType());
  llvm::AllocaInst *alloca =
      builder.CreateAlloca(var_type, nullptr, loop_var.getID());
  decls[&loop_var] = alloca;

  // Handle range literals properly
  auto range_literal = llvm::dyn_cast<RangeLiteral>(&stmt.getRange());
  if (!range_literal) {
    throw std::runtime_error("For loops currently only support range literals");
  }

  // Evaluate and store the start value
  range_literal->getStart().accept(*this);
  llvm::Value *start_val = curValue;
  builder.CreateStore(start_val, alloca);

  // Jump to cond block
  builder.CreateBr(CondBlock);

  // Generate cond block
  builder.SetInsertPoint(CondBlock);
  llvm::Value *current_val = builder.CreateLoad(var_type, alloca, "loop_var");

  // Evaluate the end value for comparison
  range_literal->getEnd().accept(*this);
  llvm::Value *end_val = curValue;

  // Create cond: current < end (for exclusive range) or current <= end
  // (for inclusive)
  llvm::Value *cond;
  if (range_literal->isInclusive()) {
    cond = builder.CreateICmpSLE(current_val, end_val, "loopcond");
  } else {
    cond = builder.CreateICmpSLT(current_val, end_val, "loopcond");
  }

  cond = builder.CreateICmpNE(cond, llvm::ConstantInt::get(cond->getType(), 0),
                              "tobool");

  builder.CreateCondBr(cond, BodyBlock, ExitBlock);
  // Generate body block
  LoopStack.emplace_back(ExitBlock, IncBlock);
  builder.SetInsertPoint(BodyBlock);
  for (auto &body_stmt : stmt.getBody().getStmts()) {
    body_stmt->accept(*this);
  }
  // Branch to increment if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(IncBlock);
  }

  // Generate increment block
  builder.SetInsertPoint(IncBlock);
  llvm::Value *incremented = builder.CreateAdd(
      current_val, llvm::ConstantInt::get(var_type, 1), "inc");
  builder.CreateStore(incremented, alloca);
  builder.CreateBr(CondBlock);

  // Pop loop context before setting exit block
  LoopStack.pop_back();

  // Continue with exit block
  builder.SetInsertPoint(ExitBlock);
}

void phi::CodeGen::visit(phi::DeclStmt &stmt) {
  VarDecl &Decl = stmt.getDecl();

  // Create allocation for the variable
  llvm::Type *Ty = getType(Decl.getType());
  llvm::AllocaInst *Alloca = builder.CreateAlloca(Ty, nullptr, Decl.getID());

  // Store in declarations map
  decls[&Decl] = Alloca;

  // If there's an initializer, evaluate it and store
  if (Decl.hasInit()) {
    Decl.getInit().accept(*this);
    builder.CreateStore(curValue, Alloca);
  }
}

void phi::CodeGen::visit(phi::BreakStmt &stmt) {
  (void)stmt;
  assert(!LoopStack.empty() && "Break statement used outside of loop. "
                               "LoopStack should also not be empty");

  builder.CreateBr(LoopStack.back().BreakTarget);

  // Create a dummy block and set insert point to it to avoid invalid IR
  llvm::BasicBlock *dummy = llvm::BasicBlock::Create(
      context, "after_break", builder.GetInsertBlock()->getParent());
  builder.SetInsertPoint(dummy);
}

void phi::CodeGen::visit(phi::ContinueStmt &stmt) {
  (void)stmt;
  assert(!LoopStack.empty() && "Continuestatement used outside of loop. "
                               "LoopStack should also not be empty");

  builder.CreateBr(LoopStack.back().ContinueTarget);

  llvm::BasicBlock *dummy = llvm::BasicBlock::Create(
      context, "after_continue", builder.GetInsertBlock()->getParent());
  builder.SetInsertPoint(dummy);
}
