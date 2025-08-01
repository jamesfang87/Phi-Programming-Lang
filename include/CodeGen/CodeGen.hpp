#include "AST/Decl.hpp"
#include <llvm/IR/Function.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/Type.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/TargetParser/Triple.h>
#include <memory>
#include <string_view>
#include <vector>

namespace phi {

class CodeGen : public ASTVisitor<void> {
public:
    CodeGen(std::vector<std::unique_ptr<FunDecl>> ast, std::string_view path)
        : ast(std::move(ast)),
          context(),
          builder(context),
          module(path, context) {
        module.setSourceFileName(path);
        module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
    }

private:
    std::vector<std::unique_ptr<FunDecl>> ast;

    llvm::LLVMContext context;
    llvm::IRBuilder<> builder;
    llvm::Module module;

    llvm::Type* gen_type(const Type& type);

    void visit(phi::IntLiteral& expr) override;
};

} // namespace phi
