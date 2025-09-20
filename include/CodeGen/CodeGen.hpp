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
  llvm::Value *visit(StructLiteral &E);
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
  std::string Path;

  std::vector<std::unique_ptr<Decl>> Ast;

  llvm::LLVMContext Context;
  llvm::IRBuilder<> Builder;
  llvm::Module Module;

  llvm::Function *CurrentFun = nullptr;
  llvm::Instruction *AllocaInsertPoint;
  std::unordered_map<Decl *, llvm::Value *> DeclMap;

  // Loop context for break/continue statements
  struct LoopContext {
    llvm::BasicBlock *BreakTarget;    // Where 'break' should jump
    llvm::BasicBlock *ContinueTarget; // Where 'continue' should jump

    LoopContext(llvm::BasicBlock *BreakBB, llvm::BasicBlock *ContinueBB)
        : BreakTarget(BreakBB), ContinueTarget(ContinueBB) {}
  };
  std::vector<LoopContext> LoopStack;

  // Defer statement management
  std::vector<std::reference_wrapper<Expr>> DeferStack;

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
  // Loop Context Management
  //===--------------------------------------------------------------------===//

  void pushLoopContext(llvm::BasicBlock *BreakBB, llvm::BasicBlock *ContinueBB);
  void popLoopContext();
  llvm::BasicBlock *getCurrentBreakTarget();
  llvm::BasicBlock *getCurrentContinueTarget();

  //===--------------------------------------------------------------------===//
  // Defer Statement Management
  //===--------------------------------------------------------------------===//

  void pushDefer(Expr &DeferredExpr);
  void executeDefers();
  void clearDefers();

  //===--------------------------------------------------------------------===//
  // Control Flow Utilities
  //===--------------------------------------------------------------------===//

  void generateTerminatorIfNeeded(llvm::BasicBlock *Target);
  bool hasTerminator() const;

  //===--------------------------------------------------------------------===//
  // Statement Generation Helper Structures
  //===--------------------------------------------------------------------===//

  struct IfStatementBlocks {
    llvm::BasicBlock *ThenBB;
    llvm::BasicBlock *ElseBB;
    llvm::BasicBlock *ExitBB;
  };

  struct WhileLoopBlocks {
    llvm::BasicBlock *CondBB;
    llvm::BasicBlock *BodyBB;
    llvm::BasicBlock *ExitBB;
  };

  struct ForLoopBlocks {
    llvm::BasicBlock *InitBB;
    llvm::BasicBlock *CondBB;
    llvm::BasicBlock *BodyBB;
    llvm::BasicBlock *IncBB;
    llvm::BasicBlock *ExitBB;
  };

  struct ForRangeInfo {
    RangeLiteral *Range;
    llvm::Value *Start;
    llvm::Value *End;
  };

  //===--------------------------------------------------------------------===//
  // Statement Generation Helper Methods
  //===--------------------------------------------------------------------===//

  // If statement generation
  IfStatementBlocks createIfStatementBlocks(IfStmt &S);
  void generateIfCondition(IfStmt &S, const IfStatementBlocks &Blocks);
  void generateIfBranches(IfStmt &S, const IfStatementBlocks &Blocks);

  // While loop generation
  WhileLoopBlocks createWhileLoopBlocks();
  void generateWhileCondition(WhileStmt &S, const WhileLoopBlocks &Blocks);
  void generateWhileBody(WhileStmt &S, const WhileLoopBlocks &Blocks);

  // For loop generation
  ForLoopBlocks createForLoopBlocks();
  ForRangeInfo extractRangeInfo(ForStmt &S);
  void generateForInit(ForStmt &S, const ForRangeInfo &RangeInfo,
                       const ForLoopBlocks &Blocks);
  void generateForCondition(ForStmt &S, const ForRangeInfo &RangeInfo,
                            const ForLoopBlocks &Blocks);
  void generateForBody(ForStmt &S, const ForLoopBlocks &Blocks);
  void generateForIncrement(ForStmt &S, const ForRangeInfo &RangeInfo,
                            const ForLoopBlocks &Blocks);

  //===--------------------------------------------------------------------===//
  // Code Generation Utilities
  //===--------------------------------------------------------------------===//

  void generateMainWrapper();
  void breakIntoBB(llvm::BasicBlock *Target);
};

} // namespace phi
