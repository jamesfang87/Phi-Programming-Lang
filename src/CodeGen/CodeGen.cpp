#include "CodeGen/CodeGen.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
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

void phi::CodeGen::visit(phi::Expr& expr) {
    // TODO: Implement expression code generation
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::visit(phi::UnaryOp& expr) {
    // TODO: Implement unary operations
    (void)expr; // Suppress unused parameter warning
}

void phi::CodeGen::generate_println_call(phi::FunCallExpr& call) {
    std::vector<llvm::Value*> args;

    if (call.get_args().empty()) {
        // No arguments - just print a newline
        llvm::Value* format = builder.CreateGlobalString("\n");
        args.push_back(format);
    } else if (call.get_args().size() == 1) {
        // Single argument - determine type and use appropriate format
        auto& arg = call.get_args()[0];
        arg->accept(*this);
        llvm::Value* arg_value = current_value;

        // Determine format string based on argument type
        llvm::Value* format;
        if (is_int_type(arg->get_type())) {
            format = builder.CreateGlobalString("%lld\n");
        } else if (is_float_type(arg->get_type())) {
            format = builder.CreateGlobalString("%g\n");
        } else {
            // Default to string format
            format = builder.CreateGlobalString("%s\n");
        }

        args.push_back(format);
        args.push_back(arg_value);
    } else {
        // Multiple arguments - first is format string, rest are values
        call.get_args()[0]->accept(*this);
        llvm::Value* format = builder.CreateGlobalString("%s\n");
        args.push_back(format);
        args.push_back(current_value);

        // Add remaining arguments
        for (size_t i = 1; i < call.get_args().size(); ++i) {
            call.get_args()[i]->accept(*this);
            args.push_back(current_value);
        }
    }

    // Call printf with the constructed arguments
    current_value = builder.CreateCall(printf_func, args);
}

void phi::CodeGen::generate_main(phi::FunDecl& main_decl) {
    // Always use int return type for main function (C compatibility)
    auto* main_type = llvm::FunctionType::get(builder.getInt32Ty(), {}, false);
    auto* main_func =
        llvm::Function::Create(main_type, llvm::Function::ExternalLinkage, "main", &module);

    // Create entry block
    auto* entry = llvm::BasicBlock::Create(context, "entry", main_func);
    builder.SetInsertPoint(entry);

    // Generate function body by visiting each statement
    for (auto& stmt : main_decl.get_block().get_stmts()) {
        stmt->accept(*this);
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
    // Automatically declare printf function for println linking
    if (!printf_func) {
        auto* char_ptr_type = llvm::PointerType::get(builder.getInt8Ty(), 0);
        auto* printf_type = llvm::FunctionType::get(builder.getInt32Ty(), {char_ptr_type}, true);
        printf_func =
            llvm::Function::Create(printf_type, llvm::Function::ExternalLinkage, "printf", &module);
    }

    // Generate functions, ignoring any user-defined println functions
    for (auto& func_decl : ast) {
        if (func_decl->get_id() == "main") {
            generate_main(*func_decl);
        } else if (func_decl->get_id() != "println") {
            generate_function(*func_decl);
        }
        // Ignore println function declarations - we link them to printf automatically
    }
}

void phi::CodeGen::generate_function(phi::FunDecl& func) {
    // Ignore println function declarations - we link them to printf automatically
    if (func.get_id() == "println") {
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
        stmt->accept(*this);
    }

    // Add return if not present for void functions
    if (return_type->isVoidTy() && !builder.GetInsertBlock()->getTerminator()) {
        builder.CreateRetVoid();
    }
}

void phi::CodeGen::generate_println_function(phi::FunDecl& func) {
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
    llvm::Value* format = builder.CreateGlobalString("%s\n");

    // Call printf
    builder.CreateCall(printf_func, {format, str_param});

    // Return void
    builder.CreateRetVoid();
}
