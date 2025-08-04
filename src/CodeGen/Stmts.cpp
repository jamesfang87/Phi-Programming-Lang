#include "AST/Expr.hpp"
#include "CodeGen/CodeGen.hpp"
#include "llvm/IR/BasicBlock.h"
#include "llvm/IR/Function.h"

void phi::CodeGen::visit(phi::ReturnStmt &stmt) {
  if (stmt.has_expr()) {
    stmt.get_expr().accept(*this);
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

  if (stmt.has_else()) {
    else_block = llvm::BasicBlock::Create(context, "if.else", function);
  }

  // Evaluate condition
  stmt.get_condition().accept(*this);
  llvm::Value *condition = curValue;

  // Branch based on condition
  if (stmt.has_else()) {
    builder.CreateCondBr(condition, then_block, else_block);
  } else {
    builder.CreateCondBr(condition, then_block, merge_block);
  }

  // Generate then block
  builder.SetInsertPoint(then_block);
  for (auto &then_stmt : stmt.get_then().get_stmts()) {
    then_stmt->accept(*this);
  }
  // Branch to merge if no terminator was created
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateBr(merge_block);
  }

  // Generate else block if it exists
  if (stmt.has_else()) {
    builder.SetInsertPoint(else_block);
    for (auto &else_stmt : stmt.get_else().get_stmts()) {
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

  // Jump to condition block
  builder.CreateBr(cond_block);

  // Generate condition block
  builder.SetInsertPoint(cond_block);
  stmt.get_condition().accept(*this);
  llvm::Value *condition = curValue;

  builder.CreateCondBr(condition, body_block, exit_block);

  // Generate body block
  builder.SetInsertPoint(body_block);
  for (auto &body_stmt : stmt.get_body().get_stmts()) {
    body_stmt->accept(*this);
  }
  // Branch back to condition if no terminator was created
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
  VarDecl &loop_var = stmt.get_loop_var();
  llvm::Type *var_type = getTy(loop_var.getTy());
  llvm::AllocaInst *alloca =
      builder.CreateAlloca(var_type, nullptr, loop_var.getID());
  decls[&loop_var] = alloca;

  // Handle range literals properly
  auto *range_literal = dynamic_cast<RangeLiteral *>(&stmt.get_range());
  if (!range_literal) {
    throw std::runtime_error("For loops currently only support range literals");
  }

  // Evaluate and store the start value
  range_literal->getStart().accept(*this);
  llvm::Value *start_val = curValue;
  builder.CreateStore(start_val, alloca);

  // Jump to condition block
  builder.CreateBr(cond_block);

  // Generate condition block
  builder.SetInsertPoint(cond_block);
  llvm::Value *current_val = builder.CreateLoad(var_type, alloca, "loop_var");

  // Evaluate the end value for comparison
  range_literal->getEnd().accept(*this);
  llvm::Value *end_val = curValue;

  // Create condition: current < end (for exclusive range) or current <= end
  // (for inclusive)
  llvm::Value *condition;
  if (range_literal->isInclusive()) {
    condition = builder.CreateICmpSLE(current_val, end_val, "loopcond");
  } else {
    condition = builder.CreateICmpSLT(current_val, end_val, "loopcond");
  }

  // Ensure condition is boolean (i1 type) - should already be from
  // CreateICmpSLT, but safety check
  if (!condition->getType()->isIntegerTy(1)) {
    // Convert to boolean by comparing with zero
    if (condition->getType()->isIntegerTy()) {
      condition = builder.CreateICmpNE(
          condition, llvm::ConstantInt::get(condition->getType(), 0), "tobool");
    } else if (condition->getType()->isFloatingPointTy()) {
      condition = builder.CreateFCmpONE(
          condition, llvm::ConstantFP::get(condition->getType(), 0.0),
          "tobool");
    }
  }

  builder.CreateCondBr(condition, body_block, exit_block);

  // Generate body block
  builder.SetInsertPoint(body_block);
  for (auto &body_stmt : stmt.get_body().get_stmts()) {
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
