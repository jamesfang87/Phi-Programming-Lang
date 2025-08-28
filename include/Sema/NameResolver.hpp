#pragma once

#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "AST/Type.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/SymbolTable.hpp"

namespace phi {

class NameResolver {
public:
  explicit NameResolver(std::vector<std::unique_ptr<Decl>> Ast,
                        std::shared_ptr<DiagnosticManager> DiagnosticsMan)
      : Ast(std::move(Ast)), DiagnosticsMan(std::move(DiagnosticsMan)) {}

  std::pair<bool, std::vector<std::unique_ptr<Decl>>> resolveNames();

  // DECLARATION VISITORS
  bool visit(FunDecl *Fun);
  bool visit(ParamDecl *Param);
  bool visit(StructDecl *Struct);
  bool visit(FieldDecl *Field);

  // EXPRESSION VISITORS
  bool visit(IntLiteral &Expression);
  bool visit(FloatLiteral &Expression);
  bool visit(StrLiteral &Expression);
  bool visit(CharLiteral &Expression);
  bool visit(BoolLiteral &Expression);
  bool visit(RangeLiteral &Expression);
  bool visit(DeclRefExpr &Expression);
  bool visit(FunCallExpr &Expression);
  bool visit(BinaryOp &Expression);
  bool visit(UnaryOp &Expression);
  bool visit(StructInitExpr &Expression);
  bool visit(FieldInitExpr &Expression);
  bool visit(MemberAccessExpr &Expression);
  bool visit(MemberFunCallExpr &Expression);

  // STATEMENT VISITORS
  bool visit(ReturnStmt &Statement);
  bool visit(IfStmt &Statement);
  bool visit(WhileStmt &Statement);
  bool visit(ForStmt &Statement);
  bool visit(DeclStmt &Statement);
  bool visit(BreakStmt &Statement);
  bool visit(ContinueStmt &Statement);
  bool visit(Expr &Statement);

private:
  /// The AST being analyzed (function declarations)
  std::vector<std::unique_ptr<Decl>> Ast;
  SymbolTable SymbolTab;
  FunDecl *CurFun = nullptr;
  std::shared_ptr<DiagnosticManager> DiagnosticsMan;

  void emitError(Diagnostic &&diagnostic) { DiagnosticsMan->emit(diagnostic); }
  void emitRedefinitionError(std::string_view SymbolKind, Decl *FirstDecl,
                             Decl *Redecl);

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
  // Check if the type has been defined yet
  bool resolveType(std::optional<Type> Type);
};

} // namespace phi
