#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"
#include "llvm/IR/Function.h"

void phi::CodeGen::visit(phi::ReturnStmt &stmt) {
  if (stmt.hasExpr()) {
    stmt.getExpr().accept(*this);
    builder.CreateRet(curValue);
  } else {
    builder.CreateRetVoid();
  }
}

void phi::CodeGen::visit(phi::IfStmt &stmt) {
  // Get current function
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Create basic blocks
  llvm::BasicBlock *then_block =
      llvm::BasicBlock::Create(context, "if.then", function);
  llvm::BasicBlock *else_block = nullptr;
  llvm::BasicBlock *merge_block =
      llvm::BasicBlock::Create(context, "if.end", function);

  if (stmt.hasElse()) {
    else_block = llvm::BasicBlock::Create(context, "if.else", function);
  }

  // Evaluate cond
  stmt.getCond().accept(*this);
  llvm::Value *cond = curValue;

  // Branch based on cond
  if (stmt.hasElse()) {
    builder.CreateCondBr(cond, then_block, else_block);
  } else {
    builder.CreateCondBr(cond, then_block, merge_block);
  }

  // Generate then block
  builder.SetInsertPoint(then_block);
  for (auto &then_stmt : stmt.getThen().getStmts()) {
    then_stmt->accept(*this);
  }
  // Branch to merge if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(merge_block);
  }

  // Generate else block if it exists
  if (stmt.hasElse()) {
    builder.SetInsertPoint(else_block);
    for (auto &else_stmt : stmt.getElse().getStmts()) {
      else_stmt->accept(*this);
    }
    // Branch to merge if no terminator was created
    if (!builder.GetInsertBlock()->getTerminator()) {
      builder.CreateBr(merge_block);
    }
  }

  // Continue with merge block
  builder.SetInsertPoint(merge_block);
}

void phi::CodeGen::visit(phi::WhileStmt &stmt) {
  // Get current function
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Create basic blocks
  llvm::BasicBlock *cond_block =
      llvm::BasicBlock::Create(context, "while.cond", function);
  llvm::BasicBlock *body_block =
      llvm::BasicBlock::Create(context, "while.body", function);
  llvm::BasicBlock *exit_block =
      llvm::BasicBlock::Create(context, "while.end", function);

  // Jump to cond block
  builder.CreateBr(cond_block);

  // Generate cond block
  builder.SetInsertPoint(cond_block);
  stmt.getCond().accept(*this);
  llvm::Value *cond = curValue;

  builder.CreateCondBr(cond, body_block, exit_block);

  // Generate body block
  builder.SetInsertPoint(body_block);
  for (auto &body_stmt : stmt.getBody().getStmts()) {
    body_stmt->accept(*this);
  }
  // Branch back to cond if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(cond_block);
  }

  // Continue with exit block
  builder.SetInsertPoint(exit_block);
}

void phi::CodeGen::visit(phi::ForStmt &stmt) {
  // Get current function
  llvm::Function *function = builder.GetInsertBlock()->getParent();

  // Create basic blocks
  llvm::BasicBlock *init_block =
      llvm::BasicBlock::Create(context, "for.init", function);
  llvm::BasicBlock *cond_block =
      llvm::BasicBlock::Create(context, "for.cond", function);
  llvm::BasicBlock *body_block =
      llvm::BasicBlock::Create(context, "for.body", function);
  llvm::BasicBlock *inc_block =
      llvm::BasicBlock::Create(context, "for.inc", function);
  llvm::BasicBlock *exit_block =
      llvm::BasicBlock::Create(context, "for.end", function);

  // Jump to initialization block
  builder.CreateBr(init_block);

  // Generate initialization block
  builder.SetInsertPoint(init_block);

  // Create allocation for loop variable
  VarDecl &loop_var = stmt.getLoopVar();
  llvm::Type *var_type = getTy(loop_var.getTy());
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
  builder.CreateBr(cond_block);

  // Generate cond block
  builder.SetInsertPoint(cond_block);
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

  // Ensure cond is boolean (i1 type) - should already be from
  // CreateICmpSLT, but safety check
  if (!cond->getType()->isIntegerTy(1)) {
    // Convert to boolean by comparing with zero
    if (cond->getType()->isIntegerTy()) {
      cond = builder.CreateICmpNE(
          cond, llvm::ConstantInt::get(cond->getType(), 0), "tobool");
    } else if (cond->getType()->isFloatingPointTy()) {
      cond = builder.CreateFCmpONE(
          cond, llvm::ConstantFP::get(cond->getType(), 0.0), "tobool");
    }
  }

  builder.CreateCondBr(cond, body_block, exit_block);

  // Generate body block
  builder.SetInsertPoint(body_block);
  for (auto &body_stmt : stmt.getBody().getStmts()) {
    body_stmt->accept(*this);
  }
  // Branch to increment if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(inc_block);
  }

  // Generate increment block
  builder.SetInsertPoint(inc_block);
  llvm::Value *incremented = builder.CreateAdd(
      current_val, llvm::ConstantInt::get(var_type, 1), "inc");
  builder.CreateStore(incremented, alloca);
  builder.CreateBr(cond_block);

  // Continue with exit block
  builder.SetInsertPoint(exit_block);
}

void phi::CodeGen::visit(phi::LetStmt &stmt) {
  VarDecl &var_decl = stmt.getDecl();

  // Create allocation for the variable
  llvm::Type *var_type = getTy(var_decl.getTy());
  llvm::AllocaInst *alloca =
      builder.CreateAlloca(var_type, nullptr, var_decl.getID());

  // Store in declarations map
  decls[&var_decl] = alloca;

  // If there's an initializer, evaluate it and store
  if (var_decl.hasInit()) {
    var_decl.getInit().accept(*this);
    builder.CreateStore(curValue, alloca);
  }
}
