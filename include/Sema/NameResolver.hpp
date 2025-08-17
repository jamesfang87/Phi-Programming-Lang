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

class NameResolver final : public ASTVisitor<bool> {
public:
  explicit NameResolver(std::vector<std::unique_ptr<Decl>> Ast)
      : Ast(std::move(Ast)) {}

  std::pair<bool, std::vector<std::unique_ptr<Decl>>> resolveNames();

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
  bool visit(Expr &Statement) override;

private:
  /// The AST being analyzed (function declarations)
  std::vector<std::unique_ptr<Decl>> Ast;

  /// Symbol table for identifier resolution
  SymbolTable SymbolTab;

  /// Pointer to the function currently being analyzed
  FunDecl *CurFun = nullptr;

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
  bool resolveFieldDecl(FieldDecl *Field);

  // Check if the type has been defined yet
  bool resolveTy(std::optional<Type> Type);
};

} // namespace phi
