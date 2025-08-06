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

class CodeGen : ASTVisitor<void> {
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

  struct LoopContext {
    llvm::BasicBlock *BreakTarget;
    llvm::BasicBlock *ContinueTarget; // for `continue`, optional
  };

  std::vector<LoopContext> LoopStack; // member of CodeGen

  llvm::Type *getType(const Type &type);
  void generateFun(phi::FunDecl &fun);
  void generateMain(phi::FunDecl &decl);
  void generatePrintlnCall(phi::FunCallExpr &call);

  void generateSintOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);
  void generateUintOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);
  void generateFloatOp(llvm::Value *lhs, llvm::Value *rhs, phi::BinaryOp &expr);

  // Visitor methods for expressions
  void visit(phi::IntLiteral &expr);
  void visit(phi::FloatLiteral &expr);
  void visit(phi::StrLiteral &expr);
  void visit(phi::CharLiteral &expr);
  void visit(phi::BoolLiteral &expr);
  void visit(phi::RangeLiteral &expr);
  void visit(phi::DeclRefExpr &expr);
  void visit(phi::FunCallExpr &expr);
  void visit(phi::BinaryOp &expr);
  void visit(phi::UnaryOp &expr);

  // Visitor methods for statements
  void visit(phi::ReturnStmt &stmt);
  void visit(phi::IfStmt &stmt);
  void visit(phi::WhileStmt &stmt);
  void visit(phi::ForStmt &stmt);
  void visit(phi::DeclStmt &stmt);
  void visit(phi::BreakStmt &stmt);
  void visit(phi::ContinueStmt &stmt);
  void visit(phi::Expr &expr);
};

} // namespace phi
