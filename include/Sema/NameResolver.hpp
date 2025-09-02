#pragma once

#include <cstdint>
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
#include "SrcManager/SrcLocation.hpp"

namespace phi {

class NameResolver {
public:
  explicit NameResolver(std::vector<std::unique_ptr<Decl>> Ast,
                        std::shared_ptr<DiagnosticManager> DiagnosticsMan)
      : Ast(std::move(Ast)), DiagnosticsMan(std::move(DiagnosticsMan)) {}

  std::pair<bool, std::vector<std::unique_ptr<Decl>>> resolveNames();

  // DECLARATION VISITORS
  bool visit(FunDecl *F);
  bool visit(ParamDecl *P);
  bool visit(StructDecl *S);
  bool visit(FieldDecl *F);

  // EXPRESSION VISITORS
  bool visit(IntLiteral &E);
  bool visit(FloatLiteral &E);
  bool visit(StrLiteral &E);
  bool visit(CharLiteral &E);
  bool visit(BoolLiteral &E);
  bool visit(RangeLiteral &E);
  bool visit(DeclRefExpr &E);
  bool visit(FunCallExpr &E);
  bool visit(BinaryOp &E);
  bool visit(UnaryOp &E);
  bool visit(StructInitExpr &E);
  bool visit(FieldInitExpr &E);
  bool visit(MemberAccessExpr &E);
  bool visit(MemberFunCallExpr &E);

  // STATEMENT VISITORS
  bool visit(ReturnStmt &S);
  bool visit(DeferStmt &S);
  bool visit(IfStmt &S);
  bool visit(WhileStmt &S);
  bool visit(ForStmt &S);
  bool visit(DeclStmt &S);
  bool visit(BreakStmt &S);
  bool visit(ContinueStmt &S);
  bool visit(Expr &S);

private:
  /// The AST being analyzed (function declarations)
  std::vector<std::unique_ptr<Decl>> Ast;
  SymbolTable SymbolTab;
  FunDecl *CurFun = nullptr;
  std::shared_ptr<DiagnosticManager> DiagnosticsMan;

  void emitError(Diagnostic &&diagnostic) { DiagnosticsMan->emit(diagnostic); }
  void emitRedefinitionError(std::string_view SymbolKind, Decl *FirstDecl,
                             Decl *Redecl);

  enum class NotFoundErrorKind : uint8_t {
    Variable,
    Function,
    Type,
    Struct,
    Field,
  };

  template <typename SrcLocation>
  void emitNotFoundError(NotFoundErrorKind Kind, std::string_view PrimaryId,
                         const SrcLocation &PrimaryLoc,
                         std::optional<std::string> ContextId = std::nullopt) {
    switch (Kind) {
    case NotFoundErrorKind::Variable:
      emitVariableNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Function:
      emitFunctionNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Type:
      emitTypeNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Struct:
      emitStructNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Field:
      emitFieldNotFound(PrimaryId, PrimaryLoc, ContextId);
      break;
    }
  }

  void emitVariableNotFound(std::string_view VarId, const SrcLocation &Loc);
  void emitFunctionNotFound(std::string_view FunId, const SrcLocation &Loc);
  void emitTypeNotFound(std::string_view TypeName, const SrcLocation &Loc);
  void emitStructNotFound(std::string_view StructId, const SrcLocation &Loc);
  void emitFieldNotFound(std::string_view FieldId, const SrcLocation &RefLoc,
                         const std::optional<std::string> &StructId);
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
