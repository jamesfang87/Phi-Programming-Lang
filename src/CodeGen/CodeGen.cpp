#include "CodeGen/CodeGen.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include "llvm/Support/Casting.h"
#include <stdexcept>

void phi::CodeGen::visit(phi::Expr &expr) {
  // TODO: Implement expression code generation
  (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::UnaryOp &expr) {
  // TODO: Implement unary operations
  (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::generatePrintlnCall(phi::FunCallExpr &call) {
  std::vector<llvm::Value *> args;

  if (call.getArgs().empty()) {
    // No arguments - just print a newline
    llvm::Value *format = Builder.CreateGlobalString("\n");
    args.push_back(format);
  } else if (call.getArgs().size() == 1) {
    // Single argument - determine type and use appropriate format
    auto &arg = call.getArgs()[0];
    arg->accept(*this);
    llvm::Value *arg_value = CurValue;

    // Determine format string based on argument type
    llvm::Value *format;
    if (arg->getType().isInteger()) {
      format = Builder.CreateGlobalString("%lld\n");
    } else if (arg->getType().isFloat()) {
      format = Builder.CreateGlobalString("%g\n");
    } else {
      // Default to string format
      format = Builder.CreateGlobalString("%s\n");
    }

    args.push_back(format);
    args.push_back(arg_value);
  } else {
    // Multiple arguments - first is format string, rest are values
    call.getArgs()[0]->accept(*this);
    llvm::Value *format = Builder.CreateGlobalString("%s\n");
    args.push_back(format);
    args.push_back(CurValue);

    // Add remaining arguments
    for (size_t i = 1; i < call.getArgs().size(); ++i) {
      call.getArgs()[i]->accept(*this);
      args.push_back(CurValue);
    }
  }

  // Call printf with the constructed arguments
  CurValue = Builder.CreateCall(PrintFun, args);
}

void phi::CodeGen::generateMain(phi::FunDecl &main_decl) {
  // Always use int return type for main function (C compatibility)
  auto *main_type = llvm::FunctionType::get(Builder.getInt32Ty(), {}, false);
  auto *main_func = llvm::Function::Create(
      main_type, llvm::Function::ExternalLinkage, "main", &Module);

  // Create entry block
  auto *entry = llvm::BasicBlock::Create(Context, "entry", main_func);
  Builder.SetInsertPoint(entry);

  // Generate function body by visiting each statement
  for (auto &stmt : main_decl.getBody().getStmts()) {
    stmt->accept(*this);
  }

  // Add return 0 if not present for main function
  if (!Builder.GetInsertBlock()->getTerminator()) {
    Builder.CreateRet(llvm::ConstantInt::get(Builder.getInt32Ty(), 0));
  }
}

void phi::CodeGen::outputIR(const std::string &filename) {
  std::error_code ec;
  llvm::raw_fd_ostream file(filename, ec);
  if (ec) {
    throw std::runtime_error("Could not open file for writing: " + filename);
  }
  Module.print(file, nullptr);
}

void phi::CodeGen::generate() {
  // Automatically declare printf function for println linking
  if (!PrintFun) {
    auto *char_ptr_type = llvm::PointerType::get(Builder.getInt8Ty(), 0);
    auto *printf_type =
        llvm::FunctionType::get(Builder.getInt32Ty(), {char_ptr_type}, true);
    PrintFun = llvm::Function::Create(
        printf_type, llvm::Function::ExternalLinkage, "printf", &Module);
  }

  // Generate functions, ignoring any user-defined println functions
  for (auto &decl : Ast) {
    auto func_decl = llvm::dyn_cast<FunDecl>(decl.get());

    if (func_decl->getId() == "main") {
      generateMain(*func_decl);
    } else if (func_decl->getId() != "println") {
      generateFun(*func_decl);
    }
  }
}

void phi::CodeGen::generateFun(phi::FunDecl &func) {
  // Ignore println function declarations - we link them to printf automatically
  if (func.getId() == "println") {
    return;
  }

  // Create function type
  std::vector<llvm::Type *> param_types;
  for (auto &param : func.getParams()) {
    param_types.push_back(param->getType().toLLVM(Context));
  }
  auto *return_type = func.getReturnTy().toLLVM(Context);
  auto *func_type = llvm::FunctionType::get(return_type, param_types, false);

  // Create function
  auto *llvm_func = llvm::Function::Create(
      func_type, llvm::Function::ExternalLinkage, func.getId(), &Module);

  // Create entry block
  auto *entry = llvm::BasicBlock::Create(Context, "entry", llvm_func);
  Builder.SetInsertPoint(entry);

  // Create allocations for parameters and store argument values
  auto arg_it = llvm_func->arg_begin();
  for (auto &param : func.getParams()) {
    // Create allocation for parameter
    llvm::Type *param_type = param->getType().toLLVM(Context);
    llvm::AllocaInst *alloca =
        Builder.CreateAlloca(param_type, nullptr, param->getId());

    // Store argument value into allocation
    Builder.CreateStore(&(*arg_it), alloca);

    // Store allocation in declarations map
    Decls[param.get()] = alloca;
    ++arg_it;
  }

  // Generate function body by visiting each statement
  for (auto &stmt : func.getBody().getStmts()) {
    stmt->accept(*this);
  }

  // Add return if not present for void functions
  if (return_type->isVoidTy() && !Builder.GetInsertBlock()->getTerminator()) {
    Builder.CreateRetVoid();
  }
}
