#pragma once

#include <cstdint>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include "AST/ASTVisitor.hpp"
#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include "Sema/SymbolTable.hpp"

namespace phi {

/**
 * @brief Semantic analyzer for the Phi programming language
 *
 * Performs semantic analysis on the AST including:
 * 1. Symbol resolution (binding identifiers to declarations)
 * 2. Type checking and validation
 * 3. Type inference for Expressionessions
 * 4. Control flow validation
 * 5. Error reporting for semantic violations
 *
 * Implements the visitor pattern to traverse and annotate the AST.
 */
class Sema final : public ASTVisitor<bool> {
public:
  /**
   * @brief Constructs a semantic analyzer
   *
   * @param ast The abstract syntax tree to analyze
   */
  explicit Sema(std::vector<std::unique_ptr<Decl>> Ast) : Ast(std::move(Ast)) {}

  /**
   * @brief Main entry point for semantic analysis
   *
   * @return std::pair<bool, std::vector<std::unique_ptr<FunDecl>>>
   *         First element: true if analysis succeeded without errors,
   *                      false if semantic errors were found
   *         Second element: The annotated AST (modified in-place)
   */
  std::pair<bool, std::vector<std::unique_ptr<Decl>>> resolveAST();

  // EXPRESSION VISITORS
  bool visit(IntLiteral &Expression) override;
  bool visit(FloatLiteral &Expression) override;
  bool visit(StrLiteral &Expression) override;
  bool visit(CharLiteral &Expression) override;
  bool visit(BoolLiteral &Expression) override;
  bool visit(RangeLiteral &Expression) override;
  bool visit(DeclRefExpr &Expression) override;
  bool visit(FunCallExpr &Expression) override;
  bool visit(BinaryOp &Expression) override;
  bool visit(UnaryOp &Expression) override;
  bool visit(StructInitExpr &Expression) override;
  bool visit(FieldInitExpr &Expression) override;
  bool visit(MemberAccessExpr &Expression) override;
  bool visit(MemberFunCallExpr &Expression) override;

  bool visit(ReturnStmt &Statement) override;
  bool visit(IfStmt &Statement) override;
  bool visit(WhileStmt &Statement) override;
  bool visit(ForStmt &Statement) override;
  bool visit(DeclStmt &Statement) override;
  bool visit(BreakStmt &Statement) override;
  bool visit(ContinueStmt &Statement) override;

  /**
   * @brief Validates Expression statements
   *
   * Primarily ensures Expression is valid and has side effects
   *
   * @param Statement The Expressionession statement
   * @return true if validation succeeded, false otherwise
   */
  bool visit(Expr &Statement) override;

private:
  /// The AST being analyzed (function declarations)
  std::vector<std::unique_ptr<Decl>> Ast;

  /// Symbol table for identifier resolution
  SymbolTable SymbolTab;

  /// Pointer to the function currently being analyzed
  FunDecl *CurFun = nullptr;

  int64_t LoopDepth = 0;
  class LoopContextRAII {
    int64_t &Depth;

  public:
    explicit LoopContextRAII(int64_t &Depth) : Depth(Depth) { ++this->Depth; }
    ~LoopContextRAII() { --Depth; }
  };

  /**
   * @brief Resolves a block statement
   *
   * @param block The block to resolve
   * @param scope_created True if new scope was already created
   * @return true if resolution succeeded, false otherwise
   */
  bool resolveBlock(Block &Block, bool ScopeCreated);

  // DECLARATION RESOLUTION
  /**
   * @brief Resolves a function declaration
   *
   * Processes parameters, return type, and body
   *
   * @param fun The function declaration
   * @return true if resolution succeeded, false otherwise
   */
  bool resolveFunDecl(FunDecl *Fun);
  bool resolveParamDecl(ParamDecl *Param);

  bool resolveStructDecl(StructDecl *Struct);
  bool resolveFieldDecl(FieldDecl *Field, StructDecl *Parent);

  /**
   * @brief Validates a type
   *
   * @param type The type to validate
   * @return true if type is valid, false otherwise
   */
  bool resolveTy(std::optional<Type> Type);

  static Type promoteNumTys(const Type &Lhs, const Type &Rhs);
};

} // namespace phi
