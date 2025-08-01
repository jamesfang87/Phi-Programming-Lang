#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
#include <iostream>
#include <llvm/IR/Function.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/TargetParser/Triple.h>
#include <memory>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

namespace phi {

class CodeGen : public ASTVisitor<void> {
public:
    CodeGen(std::vector<std::unique_ptr<FunDecl>> ast,
            std::string_view path,
            std::string_view target_triple = "")
        : ast(std::move(ast)),
          context(),
          builder(context),
          module(path, context) {
        module.setSourceFileName(path);
        if (target_triple.empty()) {
            module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
        } else {
            module.setTargetTriple(llvm::Triple(std::string(target_triple)));
        }
    }

    void generate();
    void output_ir(const std::string& filename);

private:
    std::vector<std::unique_ptr<FunDecl>> ast;

    llvm::LLVMContext context;
    llvm::IRBuilder<> builder;
    llvm::Module module;

    std::unordered_map<Decl*, llvm::Value*> declarations;
    llvm::Value* current_value = nullptr;
    llvm::Function* printf_func = nullptr;

    llvm::Type* gen_type(const Type& type);
    void generate_function(phi::FunDecl& func);
    void generate_main_function();
    void generate_println_call(phi::FunCallExpr& call);
    void generate_println_function(phi::FunDecl& func);

    // Helper methods for traversing AST nodes
    void visit_expr(phi::Expr& expr);
    void visit_stmt(phi::Stmt& stmt);

    // Visitor methods for expressions
    void visit(phi::IntLiteral& expr) override;
    void visit(phi::FloatLiteral& expr) override;
    void visit(phi::StrLiteral& expr) override;
    void visit(phi::CharLiteral& expr) override;
    void visit(phi::BoolLiteral& expr) override;
    void visit(phi::RangeLiteral& expr) override;
    void visit(phi::DeclRefExpr& expr) override;
    void visit(phi::FunCallExpr& expr) override;
    void visit(phi::BinaryOp& expr) override;
    void visit(phi::UnaryOp& expr) override;

    // Visitor methods for statements
    void visit(phi::ReturnStmt& stmt) override;
    void visit(phi::IfStmt& stmt) override;
    void visit(phi::WhileStmt& stmt) override;
    void visit(phi::ForStmt& stmt) override;
    void visit(phi::LetStmt& stmt) override;
    void visit(phi::Expr& expr) override;
};

} // namespace phi
