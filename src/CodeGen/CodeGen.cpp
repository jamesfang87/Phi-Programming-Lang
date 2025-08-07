#include "CodeGen/CodeGen.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include <stdexcept>

llvm::Type *phi::CodeGen::getType(const phi::Type &type) {
  switch (type.primitive_type()) {
  case Type::Primitive::null:
    return llvm::Type::getVoidTy(context);
  case Type::Primitive::boolean:
    return llvm::Type::getInt1Ty(context);
  case Type::Primitive::i8:
    return llvm::Type::getInt8Ty(context);
  case Type::Primitive::i16:
    return llvm::Type::getInt16Ty(context);
  case Type::Primitive::i32:
    return llvm::Type::getInt32Ty(context);
  case Type::Primitive::i64:
    return llvm::Type::getInt64Ty(context);
  case Type::Primitive::f32:
    return llvm::Type::getFloatTy(context);
  case Type::Primitive::f64:
    return llvm::Type::getDoubleTy(context);
  case Type::Primitive::str:
    return llvm::PointerType::get(llvm::Type::getInt8Ty(context), 0);
  case Type::Primitive::character:
    return llvm::Type::getInt8Ty(context);
  case Type::Primitive::u8:
    return llvm::Type::getInt8Ty(context);
  case Type::Primitive::u16:
    return llvm::Type::getInt16Ty(context);
  case Type::Primitive::u32:
    return llvm::Type::getInt32Ty(context);
  case Type::Primitive::u64:
    return llvm::Type::getInt64Ty(context);
  case Type::Primitive::range:
    return llvm::Type::getVoidTy(context); // TODO: proper range type
  default:
    throw std::runtime_error("Unsupported type: " + type.toString());
  }
}

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
    llvm::Value *format = builder.CreateGlobalString("\n");
    args.push_back(format);
  } else if (call.getArgs().size() == 1) {
    // Single argument - determine type and use appropriate format
    auto &arg = call.getArgs()[0];
    arg->accept(*this);
    llvm::Value *arg_value = curValue;

    // Determine format string based on argument type
    llvm::Value *format;
    if (isIntTy(arg->getType())) {
      format = builder.CreateGlobalString("%lld\n");
    } else if (isFloat(arg->getType())) {
      format = builder.CreateGlobalString("%g\n");
    } else {
      // Default to string format
      format = builder.CreateGlobalString("%s\n");
    }

    args.push_back(format);
    args.push_back(arg_value);
  } else {
    // Multiple arguments - first is format string, rest are values
    call.getArgs()[0]->accept(*this);
    llvm::Value *format = builder.CreateGlobalString("%s\n");
    args.push_back(format);
    args.push_back(curValue);

    // Add remaining arguments
    for (size_t i = 1; i < call.getArgs().size(); ++i) {
      call.getArgs()[i]->accept(*this);
      args.push_back(curValue);
    }
  }

  // Call printf with the constructed arguments
  curValue = builder.CreateCall(printfFun, args);
}

void phi::CodeGen::generateMain(phi::FunDecl &main_decl) {
  // Always use int return type for main function (C compatibility)
  auto *main_type = llvm::FunctionType::get(builder.getInt32Ty(), {}, false);
  auto *main_func = llvm::Function::Create(
      main_type, llvm::Function::ExternalLinkage, "main", &module);

  // Create entry block
  auto *entry = llvm::BasicBlock::Create(context, "entry", main_func);
  builder.SetInsertPoint(entry);

  // Generate function body by visiting each statement
  for (auto &stmt : main_decl.getBlock().getStmts()) {
    stmt->accept(*this);
  }

  // Add return 0 if not present for main function
  if (!builder.GetInsertBlock()->getTerminator()) {
    builder.CreateRet(llvm::ConstantInt::get(builder.getInt32Ty(), 0));
  }
}

void phi::CodeGen::outputIR(const std::string &filename) {
  std::error_code ec;
  llvm::raw_fd_ostream file(filename, ec);
  if (ec) {
    throw std::runtime_error("Could not open file for writing: " + filename);
  }
  module.print(file, nullptr);
}

void phi::CodeGen::generate() {
  // Automatically declare printf function for println linking
  if (!printfFun) {
    auto *char_ptr_type = llvm::PointerType::get(builder.getInt8Ty(), 0);
    auto *printf_type =
        llvm::FunctionType::get(builder.getInt32Ty(), {char_ptr_type}, true);
    printfFun = llvm::Function::Create(
        printf_type, llvm::Function::ExternalLinkage, "printf", &module);
  }

  // Generate functions, ignoring any user-defined println functions
  for (auto &func_decl : ast) {
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
    param_types.push_back(getType(param->getType()));
  }

  auto *return_type = getType(func.getReturnTy());
  auto *func_type = llvm::FunctionType::get(return_type, param_types, false);

  // Create function
  auto *llvm_func = llvm::Function::Create(
      func_type, llvm::Function::ExternalLinkage, func.getId(), &module);

  // Create entry block
  auto *entry = llvm::BasicBlock::Create(context, "entry", llvm_func);
  builder.SetInsertPoint(entry);

  // Create allocations for parameters and store argument values
  auto arg_it = llvm_func->arg_begin();
  for (auto &param : func.getParams()) {
    // Create allocation for parameter
    llvm::Type *param_type = getType(param->getType());
    llvm::AllocaInst *alloca =
        builder.CreateAlloca(param_type, nullptr, param->getId());

    // Store argument value into allocation
    builder.CreateStore(&(*arg_it), alloca);

    // Store allocation in declarations map
    decls[param.get()] = alloca;
    ++arg_it;
  }

  // Generate function body by visiting each statement
  for (auto &stmt : func.getBlock().getStmts()) {
    stmt->accept(*this);
  }

  // Add return if not present for void functions
  if (return_type->isVoidTy() && !builder.GetInsertBlock()->getTerminator()) {
    builder.CreateRetVoid();
  }
}
