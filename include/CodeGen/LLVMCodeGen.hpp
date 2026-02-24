#pragma once

#include <memory>
#include <set>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include <llvm/IR/BasicBlock.h>
#include <llvm/IR/Function.h>
#include <llvm/IR/IRBuilder.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Module.h>
#include <llvm/IR/Type.h>
#include <llvm/IR/Value.h>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "AST/TypeSystem/Type.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// TypeInstantiation - Represents a specific instantiation of a generic type
//===----------------------------------------------------------------------===//

struct TypeInstantiation {
  const NamedDecl *GenericDecl;
  std::vector<TypeRef> TypeArgs;

  bool operator==(const TypeInstantiation &Other) const {
    if (GenericDecl != Other.GenericDecl)
      return false;
    if (TypeArgs.size() != Other.TypeArgs.size())
      return false;
    for (size_t I = 0; I < TypeArgs.size(); ++I) {
      if (TypeArgs[I].getPtr() != Other.TypeArgs[I].getPtr())
        return false;
    }
    return true;
  }
};

struct TypeInstantiationHash {
  std::size_t operator()(const TypeInstantiation &TI) const noexcept {
    std::size_t H = std::hash<const void *>()(TI.GenericDecl);
    for (const auto &Arg : TI.TypeArgs) {
      H ^= std::hash<const void *>()(Arg.getPtr()) + 0x9e3779b9 + (H << 6) +
           (H >> 2);
    }
    return H;
  }
};

//===----------------------------------------------------------------------===//
// LLVMCodeGen - LLVM IR code generation with monomorphization
//===----------------------------------------------------------------------===//

class CodeGen {
public:
  //===--------------------------------------------------------------------===//
  // Constructor & Main Entry Points
  //===--------------------------------------------------------------------===//

  explicit CodeGen(std::vector<ModuleDecl *> Mods,
                   std::string_view SourcePath = "module");

  /// Run the full code generation pipeline
  void generate();

  /// Output the generated IR to a file
  void outputIR(const std::string &Filename);

  /// Get the LLVM module (for testing/inspection)
  llvm::Module &getModule() { return Module; }

private:
  //===--------------------------------------------------------------------===//
  // Member Variables - Core Infrastructure
  //===--------------------------------------------------------------------===//

  std::vector<ModuleDecl *> Ast;
  std::string SourcePath;

  llvm::LLVMContext Context;
  llvm::IRBuilder<> Builder;
  llvm::Module Module;

  llvm::Function *CurrentFunction = nullptr;
  uint64_t TmpVarCounter = 0;

  //===--------------------------------------------------------------------===//
  // Type & Declaration Caches
  //===--------------------------------------------------------------------===//

  /// Cache: AST Type* -> LLVM Type*
  std::unordered_map<const Type *, llvm::Type *> TypeCache;

  /// Cache: Struct/Enum name -> LLVM StructType*
  std::unordered_map<std::string, llvm::StructType *> StructTypes;

  /// Cache: Declaration -> LLVM Value (alloca for locals)
  std::unordered_map<const Decl *, llvm::Value *> NamedValues;

  /// Cache: FunDecl* -> LLVM Function*
  std::unordered_map<const FunDecl *, llvm::Function *> Functions;

  /// Cache: MethodDecl* -> LLVM Function* (methods compiled as functions)
  std::unordered_map<const MethodDecl *, llvm::Function *> Methods;

  /// Cache: Struct name -> (field name -> field index)
  std::unordered_map<std::string, std::unordered_map<std::string, unsigned>>
      FieldIndices;

  /// Cache: Enum name -> (variant name -> discriminant value)
  std::unordered_map<std::string, std::unordered_map<std::string, unsigned>>
      VariantDiscriminants;

  /// Cache: Enum name -> (variant name -> payload type)
  std::unordered_map<std::string, std::unordered_map<std::string, llvm::Type *>>
      VariantPayloadTypes;

  //===--------------------------------------------------------------------===//
  // Monomorphization Data Structures
  //===--------------------------------------------------------------------===//

  /// Set of instantiations waiting to be processed
  std::unordered_set<TypeInstantiation, TypeInstantiationHash> Instantiations;

  /// Map: TypeInstantiation -> name of monomorphized declaration
  std::unordered_map<TypeInstantiation, std::string, TypeInstantiationHash>
      MonomorphizedNames;

  /// Substitution map: TypeArgDecl* -> concrete TypeRef
  using SubstitutionMap = std::unordered_map<const TypeArgDecl *, TypeRef>;

  SubstitutionMap CurrentSubs;

  struct MonomorphizedFun {
    const FunDecl *Fun;
    std::vector<TypeRef> Args;
    llvm::Function *Fn;
  };

  struct MonomorphizedMethod {
    const MethodDecl *Method;
    std::vector<TypeRef> Args;
    llvm::Function *Fn;
  };

  std::vector<MonomorphizedFun> MonomorphizedFunctionQueue;
  std::vector<MonomorphizedMethod> MonomorphizedMethodQueue;
  std::unordered_set<std::string> GeneratedMonomorphizedBodies;

  //===--------------------------------------------------------------------===//
  // Loop Context (for break/continue)
  //===--------------------------------------------------------------------===//

  struct LoopContext {
    llvm::BasicBlock *CondBB;  // For continue
    llvm::BasicBlock *AfterBB; // For break

    LoopContext(llvm::BasicBlock *Cond, llvm::BasicBlock *After)
        : CondBB(Cond), AfterBB(After) {}
  };
  std::vector<LoopContext> LoopStack;

  //===--------------------------------------------------------------------===//
  // Phase 1: Discovery - Find generic instantiations
  //===--------------------------------------------------------------------===//

  void discoverInstantiations();
  void discoverInModule(ModuleDecl *M);
  void discoverInFunction(FunDecl *F);
  void discoverInMethod(MethodDecl *M);
  void discoverInBlock(Block *B);
  void discoverInStmt(Stmt *S);
  void discoverInExpr(Expr *E);
  void discoverInType(TypeRef T);

  /// Record a new instantiation to be processed
  void recordInstantiation(const NamedDecl *Decl,
                           const std::vector<TypeRef> &TypeArgs);

  //===--------------------------------------------------------------------===//
  // Phase 2: Monomorphization - Generate concrete versions
  //===--------------------------------------------------------------------===//

  void monomorphize();
  void monomorphizeDecl(const TypeInstantiation &TI);
  void monomorphizeStruct(const StructDecl *S,
                          const std::vector<TypeRef> &TypeArgs);
  void monomorphizeEnum(const EnumDecl *E,
                        const std::vector<TypeRef> &TypeArgs);
  void monomorphizeFunction(const FunDecl *F,
                            const std::vector<TypeRef> &TypeArgs);
  void monomorphizeMethod(const MethodDecl *M,
                          const std::vector<TypeRef> &TypeArgs);

  /// Substitute type parameters with concrete types
  TypeRef substituteType(TypeRef T, const SubstitutionMap &Subs);

  /// Build substitution map from type parameters to concrete types
  SubstitutionMap buildSubstitutionMap(const FunDecl *Decl,
                                       const std::vector<TypeRef> &TypeArgs);
  SubstitutionMap buildSubstitutionMap(const MethodDecl *Decl,
                                       const std::vector<TypeRef> &TypeArgs);
  SubstitutionMap buildSubstitutionMap(const AdtDecl *Decl,
                                       const std::vector<TypeRef> &TypeArgs);

  /// Generate unique name for monomorphized declaration
  std::string generateMonomorphizedName(const std::string &BaseName,
                                        const std::vector<TypeRef> &TypeArgs);

  //===--------------------------------------------------------------------===//
  // Phase 3: Desugaring - Transform method calls
  //===--------------------------------------------------------------------===//

  void desugar();
  void desugarModule(ModuleDecl *M);
  void desugarFunction(FunDecl *F);
  void desugarMethod(MethodDecl *M);
  void desugarBlock(Block *B);
  void desugarStmt(Stmt *S);
  void desugarExpr(Expr *E);

  //===--------------------------------------------------------------------===//
  // Phase 4: LLVM IR Generation - Type Conversion
  //===--------------------------------------------------------------------===//

  /// Convert AST type to LLVM type
  llvm::Type *getLLVMType(TypeRef T);
  llvm::Type *getLLVMType(const Type *T);
  bool hasGenericType(TypeRef T);
  bool hasGenericType(const Type *T);

  /// Get or create LLVM struct type for a struct declaration
  llvm::StructType *getOrCreateStructType(const StructDecl *S);
  llvm::StructType *
  getOrCreateStructType(const std::string &Name,
                        const std::vector<TypeRef> &FieldTypes);

  /// Get or create LLVM struct type for an enum declaration
  /// Enum layout: { i32 discriminant, [largest_payload_size x i8] }
  llvm::StructType *getOrCreateEnumType(const EnumDecl *E);
  llvm::StructType *
  getOrCreateEnumType(const std::string &Name,
                      const std::vector<VariantDecl *> &Variants);

  /// Get the size of a type in bytes
  uint64_t getTypeSize(llvm::Type *T);

  //===--------------------------------------------------------------------===//
  // Phase 4: LLVM IR Generation - Declarations
  //===--------------------------------------------------------------------===//

  void codegenModule(ModuleDecl *M);

  /// Declare struct types (first pass)
  void declareStructTypes(ModuleDecl *M);

  /// Declare enum types (first pass)
  void declareEnumTypes(ModuleDecl *M);

  /// Declare function signatures (second pass)
  void declareFunctions(ModuleDecl *M);

  /// Generate function bodies (third pass)
  void generateFunctionBodies(ModuleDecl *M);

  /// Generate monomorphized function bodies
  void generateMonomorphizedBodies();

  /// Create function declaration
  llvm::Function *codegenFunctionDecl(FunDecl *F);
  llvm::Function *codegenMethodDecl(MethodDecl *M,
                                    const std::string &MangledName);

  /// Generate function body
  void codegenFunctionBody(FunDecl *F, llvm::Function *Fn);
  void codegenMethodBody(MethodDecl *M, llvm::Function *Fn);

  //===--------------------------------------------------------------------===//
  // Phase 4: LLVM IR Generation - Statements
  //===--------------------------------------------------------------------===//

  void codegenBlock(Block *B);
  void codegenStmt(Stmt *S);

  void codegenDeclStmt(DeclStmt *S);
  void codegenReturnStmt(ReturnStmt *S);
  void codegenIfStmt(IfStmt *S);
  void codegenWhileStmt(WhileStmt *S);
  void codegenForStmt(ForStmt *S);
  void codegenBreakStmt(BreakStmt *S);
  void codegenContinueStmt(ContinueStmt *S);
  void codegenExprStmt(ExprStmt *S);

  //===--------------------------------------------------------------------===//
  // Phase 4: LLVM IR Generation - Expressions
  //===--------------------------------------------------------------------===//

  llvm::Value *codegenExpr(Expr *E);

  // Literals
  llvm::Value *codegenIntLiteral(IntLiteral *E);
  llvm::Value *codegenFloatLiteral(FloatLiteral *E);
  llvm::Value *codegenBoolLiteral(BoolLiteral *E);
  llvm::Value *codegenStrLiteral(StrLiteral *E);
  llvm::Value *codegenCharLiteral(CharLiteral *E);
  llvm::Value *codegenTupleLiteral(TupleLiteral *E);
  llvm::Value *codegenArrayLiteral(ArrayLiteral *E);

  // References and Calls
  llvm::Value *codegenDeclRef(DeclRefExpr *E);
  llvm::Value *codegenFunCall(FunCallExpr *E);
  llvm::Value *codegenMethodCall(MethodCallExpr *E);

  // Operators
  llvm::Value *codegenBinaryOp(BinaryOp *E);
  llvm::Value *codegenUnaryOp(UnaryOp *E);

  // Struct/Enum
  llvm::Value *codegenAdtInit(AdtInit *E);
  llvm::Value *codegenStructInit(AdtInit *E, const StructDecl *S);
  llvm::Value *codegenEnumInit(AdtInit *E, const EnumDecl *En);
  llvm::Value *codegenFieldAccess(FieldAccessExpr *E);
  llvm::Value *codegenTupleIndex(TupleIndex *E);
  llvm::Value *codegenArrayIndex(ArrayIndex *E);

  // Match
  llvm::Value *codegenMatchExpr(MatchExpr *E);
  llvm::Value *codegenIntrinsicCall(IntrinsicCall *E);

  //===--------------------------------------------------------------------===//
  // Phase 4: Pattern Matching Codegen
  //===--------------------------------------------------------------------===//

  /// Generate code for a match arm, returns the result value
  llvm::Value *codegenMatchArm(const MatchExpr::Arm &Arm,
                               llvm::Value *Scrutinee,
                               llvm::BasicBlock *NextArmBB,
                               llvm::BasicBlock *MergeBB,
                               llvm::PHINode *ResultPHI);

  /// Generate pattern matching code, branches to SuccessBB or FailBB
  void matchPattern(const Pattern &Pat, llvm::Value *Scrutinee,
                    llvm::BasicBlock *SuccessBB, llvm::BasicBlock *FailBB);

  /// Match wildcard pattern (always succeeds)
  void matchWildcard(llvm::BasicBlock *SuccessBB);

  /// Match literal pattern
  void matchLiteral(const PatternAtomics::Literal &Lit, llvm::Value *Scrutinee,
                    llvm::BasicBlock *SuccessBB, llvm::BasicBlock *FailBB);

  /// Match enum variant pattern
  void matchVariant(const PatternAtomics::Variant &Var, llvm::Value *Scrutinee,
                    llvm::BasicBlock *SuccessBB, llvm::BasicBlock *FailBB);

  //===--------------------------------------------------------------------===//
  // Helpers
  //===--------------------------------------------------------------------===//

  /// Create alloca at function entry for a variable
  llvm::AllocaInst *createEntryBlockAlloca(llvm::Function *Fn,
                                           const std::string &Name,
                                           llvm::Type *Ty);

  /// Generate a unique temporary variable name
  std::string generateTempVar();

  /// Check if current block has a terminator
  bool hasTerminator() const;

  /// Get L-value pointer for an expression (for assignment)
  llvm::Value *getLValuePtr(Expr *E);

  /// Load a value from a pointer
  llvm::Value *loadValue(llvm::Value *Ptr, llvm::Type *Ty);

  /// Store a value to a pointer
  void storeValue(llvm::Value *Val, llvm::Value *Ptr);

  //===--------------------------------------------------------------------===//
  // Built-in Functions
  //===--------------------------------------------------------------------===//

  llvm::Function *PrintFn = nullptr;
  void declarePrintln();
  llvm::Value *generatePrintlnCall(FunCallExpr *Call);
};

} // namespace phi
