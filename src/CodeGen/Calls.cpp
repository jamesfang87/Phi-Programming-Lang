#include "CodeGen/CodeGen.hpp"

void phi::CodeGen::visit(phi::DeclRefExpr& expr) {
    // Look up the declaration in our declarations map
    auto it = declarations.find(expr.get_decl());
    if (it != declarations.end()) {
        current_value = it->second;
    } else {
        // Declaration not found - this might be a global or external symbol
        current_value = nullptr;
    }
}

void phi::CodeGen::visit(phi::FunCallExpr& expr) {
    auto* decl_ref = dynamic_cast<phi::DeclRefExpr*>(&expr.get_callee());

    // Temp linking to printf
    if (decl_ref->get_id() == "println") {
        generate_println_call(expr);
        return;
    }

    // Generate arguments
    std::vector<llvm::Value*> args;
    for (auto& arg : expr.get_args()) {
        arg->accept(*this);
        args.push_back(current_value);
    }

    // Find the function to call
    llvm::Function* func = module.getFunction(decl_ref->get_id());
    if (func) {
        current_value = builder.CreateCall(func, args);
    }
}
