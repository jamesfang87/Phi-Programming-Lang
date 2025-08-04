#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
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
  CodeGen(std::vector<std::unique_ptr<FunDecl>> ast, std::string_view path,
          std::string_view targetTriple = "")
      : ast(std::move(ast)), context(), builder(context),
        module(path, context) {
    module.setSourceFileName(path);
    if (targetTriple.empty()) {
      module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
    } else {
      module.setTargetTriple(llvm::Triple(std::string(targetTriple)));
    }
  }

  void generate();
  void outputIR(const std::string &filename);

private:
  std::vector<std::unique_ptr<FunDecl>> ast;

  llvm::LLVMContext context;
  llvm::IRBuilder<> builder;
  llvm::Module module;

  std::unordered_map<Decl *, llvm::Value *> decls;
  llvm::Value *curValue = nullptr;
  llvm::Value *curFun = nullptr;
  llvm::Function *printfFun = nullptr;

  llvm::Type *getTy(const Type &type);
  void generateFun(phi::FunDecl &fun);
  void generateMain(phi::FunDecl &main_decl);
  void generate_println_call(phi::FunCallExpr &call);
  void generate_println_function(phi::FunDecl &fun);

  void generateSintOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);
  void generateUintOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);
  void generateFloatOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);

  // Visitor methods for expressions
  void visit(phi::IntLiteral &expr) override;
  void visit(phi::FloatLiteral &expr) override;
  void visit(phi::StrLiteral &expr) override;
  void visit(phi::CharLiteral &expr) override;
  void visit(phi::BoolLiteral &expr) override;
  void visit(phi::RangeLiteral &expr) override;
  void visit(phi::DeclRefExpr &expr) override;
  void visit(phi::FunCallExpr &expr) override;
  void visit(phi::BinaryOp &expr) override;
  void visit(phi::UnaryOp &expr) override;

  // Visitor methods for statements
  void visit(phi::ReturnStmt &stmt) override;
  void visit(phi::IfStmt &stmt) override;
  void visit(phi::WhileStmt &stmt) override;
  void visit(phi::ForStmt &stmt) override;
  void visit(phi::LetStmt &stmt) override;
  void visit(phi::Expr &expr) override;
};

} // namespace phi
