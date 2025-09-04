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

namespace phi {

class CodeGen {
public:
  CodeGen(std::vector<std::unique_ptr<Decl>> Ast, std::string_view SourcePath,
          std::string_view TargetTriple = "");

  // pipeline
  void generate();
  void outputIR(const std::string &Filename);

  // Expression visitors -> return llvm::Value*
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
  llvm::Value *visit(MemberAccessExpr &E);
  llvm::Value *visit(MemberFunCallExpr &E);

  // Statement visitors -> emit code, return void
  void visit(ReturnStmt &S);
  void visit(DeferStmt &S);
  void visit(IfStmt &S);
  void visit(WhileStmt &S);
  void visit(ForStmt &S);
  void visit(DeclStmt &S);
  void visit(BreakStmt &S);
  void visit(ContinueStmt &S);
  void visit(ExprStmt &S);

  // Declaration visitors
  void visit(FunDecl &D);
  void visit(ParamDecl &D);
  void visit(StructDecl &D);
  void visit(FieldDecl &D);
  void visit(VarDecl &D);

private:
  // AST + LLVM
  std::vector<std::unique_ptr<Decl>> AstList;

  llvm::LLVMContext Context;
  llvm::IRBuilder<> Builder;
  llvm::Module Module;

  // Decl -> alloca / global / function
  std::unordered_map<Decl *, llvm::Value *> DeclMap;

  // Printf bridging
  llvm::Function *PrintfFn = nullptr;

  // Defer support
  llvm::Function *CurrentFunction = nullptr;
  std::unordered_map<llvm::Function *, llvm::BasicBlock *> CleanupBlockMap;
  std::unordered_map<llvm::Function *, llvm::AllocaInst *> ReturnAllocaMap;
  std::unordered_map<llvm::Function *, std::vector<Stmt *>> DeferMap;

  // Loop context
  struct LoopContext {
    llvm::BasicBlock *BreakTarget;
    llvm::BasicBlock *ContinueTarget;
  };
  std::vector<LoopContext> LoopStack;

  // Structs
  std::unordered_map<StructDecl *, llvm::StructType *> StructTypeMap;
  std::unordered_map<const FieldDecl *, unsigned> FieldIndexMap;

  // helpers
  void ensurePrintfDeclared();
  llvm::Value *generatePrintlnBridge(FunCallExpr &Call);
  llvm::Value *getAllocaForDecl(Decl *D);
  void createStructLayout(StructDecl *S);
  StructDecl *findContainingStruct(const FieldDecl *F);
  llvm::Value *computeMemberPointer(llvm::Value *BasePtr,
                                    const FieldDecl *Field);
  llvm::Value *getAddressOf(Expr *E);

  // defer helpers
  llvm::AllocaInst *ensureReturnAllocaForCurrentFunction(llvm::Type *RetTy);
  void recordDeferForCurrentFunction(Stmt *S);
  void emitDeferredForFunction(llvm::Function *F);

  // convenience wrapper used only in generate() to evaluate a top-level
  // initializer
  llvm::Value *evaluateExprUsingVisit(Expr &E) {
    // AST expression accept implementations must return llvm::Value*
    return E.accept(*this);
  }
};

} // namespace phi
