#include "CodeGen/CodeGen.hpp"
#include "AST/Expr.hpp"
#include <stdexcept>

llvm::Type* phi::CodeGen::gen_type(const phi::Type& type) {
    switch (type.primitive_type()) {
        case Type::Primitive::null: return llvm::Type::getVoidTy(context);
        case Type::Primitive::boolean: return llvm::Type::getInt1Ty(context);
        case Type::Primitive::i8: return llvm::Type::getInt8Ty(context);
        case Type::Primitive::i16: return llvm::Type::getInt16Ty(context);
        case Type::Primitive::i32: return llvm::Type::getInt32Ty(context);
        case Type::Primitive::i64: return llvm::Type::getInt64Ty(context);
        case Type::Primitive::f32: return llvm::Type::getFloatTy(context);
        case Type::Primitive::f64: return llvm::Type::getDoubleTy(context);
        case Type::Primitive::str: return llvm::PointerType::get(llvm::Type::getInt8Ty(context), 0);
        case Type::Primitive::character: return llvm::Type::getInt8Ty(context);
        case Type::Primitive::u8: return llvm::Type::getInt8Ty(context);
        case Type::Primitive::u16: return llvm::Type::getInt16Ty(context);
        case Type::Primitive::u32: return llvm::Type::getInt32Ty(context);
        case Type::Primitive::u64: return llvm::Type::getInt64Ty(context);
        case Type::Primitive::range:
            return llvm::Type::getVoidTy(context); // TODO: proper range type
        default: throw std::runtime_error("Unsupported type: " + type.to_string());
    }
}

void phi::CodeGen::visit(phi::IntLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt64Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::FloatLiteral& expr) {
    current_value = llvm::ConstantFP::get(builder.getDoubleTy(), expr.get_value());
}

void phi::CodeGen::visit(phi::StrLiteral& expr) {
    current_value = builder.CreateGlobalStringPtr(expr.get_value());
}

void phi::CodeGen::visit(phi::CharLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt8Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::BoolLiteral& expr) {
    current_value = llvm::ConstantInt::get(builder.getInt1Ty(), expr.get_value());
}

void phi::CodeGen::visit(phi::RangeLiteral& expr) {
    // TODO: Implement range literals
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::DeclRefExpr& expr) {
    // TODO: Implement declaration references
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::FunCallExpr& expr) {
    // Check if this is a function call by examining the callee
    if (auto* decl_ref = dynamic_cast<phi::DeclRefExpr*>(&expr.get_callee())) {
        // Generate arguments
        std::vector<llvm::Value*> args;
        for (auto& arg : expr.get_args()) {
            visit_expr(*arg);
            args.push_back(current_value);
        }

        // Find the function to call
        llvm::Function* func = module.getFunction(decl_ref->get_id());
        if (func) {
            current_value = builder.CreateCall(func, args);
        }
    }
}

void phi::CodeGen::visit(phi::BinaryOp& expr) {
    // TODO: Implement binary operations
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::UnaryOp& expr) {
    // TODO: Implement unary operations
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::ReturnStmt& stmt) {
    if (stmt.has_expr()) {
        visit_expr(stmt.get_expr());
        builder.CreateRet(current_value);
    } else {
        builder.CreateRetVoid();
    }
}

void phi::CodeGen::visit(phi::IfStmt& stmt) {
    // TODO: Implement if statements
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::WhileStmt& stmt) {
    // TODO: Implement while loops
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::ForStmt& stmt) {
    // TODO: Implement for loops
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::LetStmt& stmt) {
    // TODO: Implement let statements
    (void)stmt; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::Expr& expr) {
    // Default expression handler - dispatch to specific type
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit_expr(phi::Expr& expr) {
    // Dispatch to the appropriate visit method based on expression type
    if (auto* int_lit = dynamic_cast<phi::IntLiteral*>(&expr)) {
        visit(*int_lit);
    } else if (auto* float_lit = dynamic_cast<phi::FloatLiteral*>(&expr)) {
        visit(*float_lit);
    } else if (auto* str_lit = dynamic_cast<phi::StrLiteral*>(&expr)) {
        visit(*str_lit);
    } else if (auto* char_lit = dynamic_cast<phi::CharLiteral*>(&expr)) {
        visit(*char_lit);
    } else if (auto* bool_lit = dynamic_cast<phi::BoolLiteral*>(&expr)) {
        visit(*bool_lit);
    } else if (auto* range_lit = dynamic_cast<phi::RangeLiteral*>(&expr)) {
        visit(*range_lit);
    } else if (auto* decl_ref = dynamic_cast<phi::DeclRefExpr*>(&expr)) {
        visit(*decl_ref);
    } else if (auto* fun_call = dynamic_cast<phi::FunCallExpr*>(&expr)) {
        visit(*fun_call);
    } else if (auto* binary_op = dynamic_cast<phi::BinaryOp*>(&expr)) {
        visit(*binary_op);
    } else if (auto* unary_op = dynamic_cast<phi::UnaryOp*>(&expr)) {
        visit(*unary_op);
    }
}

void phi::CodeGen::visit_stmt(phi::Stmt& stmt) {
    // Dispatch to the appropriate visit method based on statement type
    if (auto* ret_stmt = dynamic_cast<phi::ReturnStmt*>(&stmt)) {
        visit(*ret_stmt);
    } else if (auto* if_stmt = dynamic_cast<phi::IfStmt*>(&stmt)) {
        visit(*if_stmt);
    } else if (auto* while_stmt = dynamic_cast<phi::WhileStmt*>(&stmt)) {
        visit(*while_stmt);
    } else if (auto* for_stmt = dynamic_cast<phi::ForStmt*>(&stmt)) {
        visit(*for_stmt);
    } else if (auto* let_stmt = dynamic_cast<phi::LetStmt*>(&stmt)) {
        visit(*let_stmt);
    } else if (auto* fun_call = dynamic_cast<phi::FunCallExpr*>(&stmt)) {
        visit(*fun_call);
    }
}

void phi::CodeGen::generate_println_call(phi::FunCallExpr& call) {
    // Create printf function if it doesn't exist
    if (!printf_func) {
        auto* char_ptr_type = llvm::PointerType::get(builder.getInt8Ty(), 0);
        auto* printf_type = llvm::FunctionType::get(builder.getInt32Ty(), {char_ptr_type}, true);
        printf_func =
            llvm::Function::Create(printf_type, llvm::Function::ExternalLinkage, "printf", &module);
    }

    // Generate the argument (should be a string)
    if (!call.get_args().empty()) {
        visit_expr(*call.get_args()[0]);
        llvm::Value* str_arg = current_value;

        // Create format string with newline
        llvm::Value* format = builder.CreateGlobalStringPtr("%s\n");

        // Call printf
        builder.CreateCall(printf_func, {format, str_arg});
    }
}

void phi::CodeGen::generate_main_function() {
    // Find the main function in the AST
    phi::FunDecl* main_func_decl = nullptr;
    for (auto& func_decl : ast) {
        if (func_decl->get_id() == "main") {
            main_func_decl = func_decl.get();
            break;
        }
    }

    if (!main_func_decl) {
        // No main function found, create a default one that returns 0
        auto* main_type = llvm::FunctionType::get(builder.getInt32Ty(), {}, false);
        auto* main_func =
            llvm::Function::Create(main_type, llvm::Function::ExternalLinkage, "main", &module);
        auto* entry = llvm::BasicBlock::Create(context, "entry", main_func);
        builder.SetInsertPoint(entry);
        builder.CreateRet(llvm::ConstantInt::get(builder.getInt32Ty(), 0));
        return;
    }

    // Generate the actual main function, but with int return type for C compatibility
    std::vector<llvm::Type*> param_types;
    for (auto& param : main_func_decl->get_params()) {
        param_types.push_back(gen_type(param->get_type()));
    }

    // Always use int return type for main function (C compatibility)
    auto* main_type = llvm::FunctionType::get(builder.getInt32Ty(), param_types, false);
    auto* main_func =
        llvm::Function::Create(main_type, llvm::Function::ExternalLinkage, "main", &module);

    // Create entry block
    auto* entry = llvm::BasicBlock::Create(context, "entry", main_func);
    builder.SetInsertPoint(entry);

    // Store parameters in declarations map
    auto arg_it = main_func->arg_begin();
    for (auto& param : main_func_decl->get_params()) {
        declarations[param.get()] = &(*arg_it);
        ++arg_it;
    }

    // Generate function body by visiting each statement
    for (auto& stmt : main_func_decl->get_block().get_stmts()) {
        visit_stmt(*stmt);
    }

    // Add return 0 if not present for main function
    if (!builder.GetInsertBlock()->getTerminator()) {
        builder.CreateRet(llvm::ConstantInt::get(builder.getInt32Ty(), 0));
    }
}

void phi::CodeGen::output_ir(const std::string& filename) {
    std::error_code ec;
    llvm::raw_fd_ostream file(filename, ec);
    if (ec) {
        throw std::runtime_error("Could not open file for writing: " + filename);
    }
    module.print(file, nullptr);
}

void phi::CodeGen::generate() {
    // First, generate all println functions
    for (auto& func_decl : ast) {
        if (func_decl->get_id() == "println") {
            generate_function(*func_decl);
        }
    }

    // Then generate other non-main functions
    for (auto& func_decl : ast) {
        if (func_decl->get_id() != "println" && func_decl->get_id() != "main") {
            generate_function(*func_decl);
        }
    }

    // Finally generate main function
    generate_main_function();
}

void phi::CodeGen::generate_function(phi::FunDecl& func) {
    // Special handling for println function
    if (func.get_id() == "println") {
        generate_println_function(func);
        return;
    }

    // Create function type
    std::vector<llvm::Type*> param_types;
    for (auto& param : func.get_params()) {
        param_types.push_back(gen_type(param->get_type()));
    }

    auto* return_type = gen_type(func.get_return_type());
    auto* func_type = llvm::FunctionType::get(return_type, param_types, false);

    // Create function
    auto* llvm_func =
        llvm::Function::Create(func_type, llvm::Function::ExternalLinkage, func.get_id(), &module);

    // Create entry block
    auto* entry = llvm::BasicBlock::Create(context, "entry", llvm_func);
    builder.SetInsertPoint(entry);

    // Store parameters in declarations map
    auto arg_it = llvm_func->arg_begin();
    for (auto& param : func.get_params()) {
        declarations[param.get()] = &(*arg_it);
        ++arg_it;
    }

    // Generate function body by visiting each statement
    for (auto& stmt : func.get_block().get_stmts()) {
        visit_stmt(*stmt);
    }

    // Add return if not present for void functions
    if (return_type->isVoidTy() && !builder.GetInsertBlock()->getTerminator()) {
        builder.CreateRetVoid();
    }
}

void phi::CodeGen::generate_println_function(phi::FunDecl& func) {
    // Create printf function if it doesn't exist
    if (!printf_func) {
        auto* char_ptr_type = llvm::PointerType::get(builder.getInt8Ty(), 0);
        auto* printf_type = llvm::FunctionType::get(builder.getInt32Ty(), {char_ptr_type}, true);
        printf_func =
            llvm::Function::Create(printf_type, llvm::Function::ExternalLinkage, "printf", &module);
    }

    // Create println function type (takes string, returns void)
    std::vector<llvm::Type*> param_types;
    for (auto& param : func.get_params()) {
        param_types.push_back(gen_type(param->get_type()));
    }

    auto* return_type = gen_type(func.get_return_type());
    auto* func_type = llvm::FunctionType::get(return_type, param_types, false);

    // Create println function
    auto* println_func =
        llvm::Function::Create(func_type, llvm::Function::ExternalLinkage, "println", &module);

    // Create entry block
    auto* entry = llvm::BasicBlock::Create(context, "entry", println_func);
    builder.SetInsertPoint(entry);

    // Get the string parameter
    auto* str_param = &*println_func->arg_begin();

    // Create format string with newline
    llvm::Value* format = builder.CreateGlobalStringPtr("%s\n");

    // Call printf
    builder.CreateCall(printf_func, {format, str_param});

    // Return void
    builder.CreateRetVoid();
}
