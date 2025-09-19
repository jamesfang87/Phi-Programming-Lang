#pragma once

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

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Stmt.hpp"
#include "llvm/IR/BasicBlock.h"

namespace phi {

//===----------------------------------------------------------------------===//
// CodeGen - LLVM IR code generation for Phi AST
//===----------------------------------------------------------------------===//

class CodeGen {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  CodeGen(std::vector<std::unique_ptr<Decl>> Ast, std::string_view SourcePath);

  //===--------------------------------------------------------------------===//
  // Main Pipeline Methods
  //===--------------------------------------------------------------------===//

  /// Generate LLVM IR from the AST
  void generate();

  /// Output the generated IR to a file
  void outputIR(const std::string &Filename);

  //===--------------------------------------------------------------------===//
  // Expression Visitor Methods -> return llvm::Value*
  //===--------------------------------------------------------------------===//

  llvm::Value *visit(Expr &E);
  llvm::Value *visit(IntLiteral &E);
  llvm::Value *visit(FloatLiteral &E);
  llvm::Value *visit(StrLiteral &E);
  llvm::Value *visit(CharLiteral &E);
  llvm::Value *visit(BoolLiteral &E);
  llvm::Value *visit(RangeLiteral &E);
  llvm::Value *visit(DeclRefExpr &E);
  llvm::Value *visit(FunCallExpr &E);
  llvm::Value *visit(BinaryOp &E);
  llvm::Value *visit(UnaryOp &E);
  llvm::Value *visit(StructInitExpr &E);
  llvm::Value *visit(FieldInitExpr &E);
  llvm::Value *visit(FieldAccessExpr &E);
  llvm::Value *visit(MethodCallExpr &E);

  //===--------------------------------------------------------------------===//
  // Statement Visitor Methods -> emit code, return void
  //===--------------------------------------------------------------------===//

  void visit(Stmt &S);
  void visit(ReturnStmt &S);
  void visit(DeferStmt &S);
  void visit(IfStmt &S);
  void visit(WhileStmt &S);
  void visit(ForStmt &S);
  void visit(DeclStmt &S);
  void visit(BreakStmt &S);
  void visit(ContinueStmt &S);
  void visit(ExprStmt &S);
  void visit(Block &B);

  //===--------------------------------------------------------------------===//
  // Declaration Visitor Methods
  //===--------------------------------------------------------------------===//

  void visit(Decl &D);
  void visit(FunDecl &D);
  void visit(ParamDecl &D);
  void visit(StructDecl &D);
  void visit(FieldDecl &D);
  void visit(VarDecl &D);

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::vector<std::unique_ptr<Decl>> Ast;

  llvm::LLVMContext Context;
  llvm::IRBuilder<> Builder;
  llvm::Module Module;

  llvm::Function *CurrentFun = nullptr;
  llvm::Instruction *AllocaInsertPoint;
  std::unordered_map<Decl *, llvm::Value *> DeclMap;

  //===--------------------------------------------------------------------===//
  // Declaration Helper Methods
  //===--------------------------------------------------------------------===//

  void declareHeader(StructDecl &D);
  void declareHeader(FunDecl &D);

  //===--------------------------------------------------------------------===//
  // Memory Management Operations
  //===--------------------------------------------------------------------===//

  llvm::AllocaInst *stackAlloca(Decl &D);
  llvm::Value *load(llvm::Value *Val, const Type &T);
  llvm::Value *store(llvm::Value *Val, llvm::Value *Destination, const Type &T);

  //===--------------------------------------------------------------------===//
  // Built-in Function Support
  //===--------------------------------------------------------------------===//

  llvm::Function *PrintFun = nullptr;
  void declarePrint();
  llvm::Value *generatePrintlnBridge(FunCallExpr &Call);

  //===--------------------------------------------------------------------===//
  // Code Generation Utilities
  //===--------------------------------------------------------------------===//

  void generateMainWrapper();
  void breakIntoBB(llvm::BasicBlock *Target);

  //===--------------------------------------------------------------------===//
  // Loop Context Management
  //===--------------------------------------------------------------------===//

  struct LoopContext {
    llvm::BasicBlock *BreakTarget;
    llvm::BasicBlock *ContinueTarget;
  };
  std::vector<LoopContext> LoopStack;
};

} // namespace phi
