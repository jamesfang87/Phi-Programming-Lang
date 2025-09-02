#include <memory>
#include <string>
#include <string_view>
#include <unordered_map>
#include <vector>

#include <llvm/IR/Function.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/TargetParser/Host.h>
#include <llvm/TargetParser/Triple.h>

#include "AST/Decl.hpp"
#include "AST/Stmt.hpp"

namespace phi {

class CodeGen {
public:
  CodeGen(std::vector<std::unique_ptr<Decl>> Ast, std::string_view Path,
          std::string_view TargetTriple = "")
      : Ast(std::move(Ast)), Context(), Builder(Context),
        Module(Path, Context) {
    Module.setSourceFileName(Path);
    if (TargetTriple.empty()) {
      Module.setTargetTriple(llvm::Triple(llvm::sys::getDefaultTargetTriple()));
    } else {
      Module.setTargetTriple(llvm::Triple(std::string(TargetTriple)));
    }
  }

  void generate();
  void outputIR(const std::string &Filename);

  // Visitor methods for expressions
  void visit(phi::IntLiteral &E);
  void visit(phi::FloatLiteral &E);
  void visit(phi::StrLiteral &E);
  void visit(phi::CharLiteral &E);
  void visit(phi::BoolLiteral &E);
  void visit(phi::RangeLiteral &E);
  void visit(phi::DeclRefExpr &E);
  void visit(phi::FunCallExpr &E);
  void visit(phi::BinaryOp &E);
  void visit(phi::UnaryOp &E);

  // Visitor methods for statements
  void visit(phi::ReturnStmt &S);
  void visit(phi::DeferStmt &S);
  void visit(phi::IfStmt &S);
  void visit(phi::WhileStmt &S);
  void visit(phi::ForStmt &S);
  void visit(phi::DeclStmt &S);
  void visit(phi::BreakStmt &S);
  void visit(phi::ContinueStmt &S);
  void visit(phi::Expr &S);

private:
  std::vector<std::unique_ptr<Decl>> Ast;

  llvm::LLVMContext Context;
  llvm::IRBuilder<> Builder;
  llvm::Module Module;

  std::unordered_map<Decl *, llvm::Value *> Decls;
  llvm::Value *CurValue = nullptr;
  llvm::Value *CurFun = nullptr;
  llvm::Function *PrintFun = nullptr;

  struct LoopContext {
    llvm::BasicBlock *BreakTarget;
    llvm::BasicBlock *ContinueTarget; // for `continue`, optional
  };

  std::vector<LoopContext> LoopStack; // member of CodeGen

  void generateFun(phi::FunDecl &Fun);
  void generateMain(phi::FunDecl &MainFun);
  void generatePrintlnCall(phi::FunCallExpr &Call);

  void generateSintOp(llvm::Value *Lhs, llvm::Value *Rhs, phi::BinaryOp &E);
  void generateUintOp(llvm::Value *Lhs, llvm::Value *Rhs, phi::BinaryOp &E);
  void generateFloatOp(llvm::Value *Lhs, llvm::Value *Rhs, phi::BinaryOp &E);
};

} // namespace phi
