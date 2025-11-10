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

//===----------------------------------------------------------------------===//
// NameResolver - Name resolution and symbol binding for Phi AST
//===----------------------------------------------------------------------===//

class NameResolver {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  explicit NameResolver(std::vector<std::unique_ptr<Decl>> Ast,
                        std::shared_ptr<DiagnosticManager> DiagnosticsMan)
      : Ast(std::move(Ast)), Diags(std::move(DiagnosticsMan)) {}

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::pair<bool, std::vector<std::unique_ptr<Decl>>> resolveNames();

  //===--------------------------------------------------------------------===//
  // Type Visitor Method -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(std::optional<Type> MaybeType);
  bool resolveTypeByName(const phi::Type &Type, const std::string &Name);

  //===--------------------------------------------------------------------===//
  // Declaration Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(FunDecl *D);
  bool visit(ParamDecl *D);
  bool visit(StructDecl *D);
  bool visit(FieldDecl *D);
  bool visit(EnumDecl *D);
  bool visit(VariantDecl *D);

  //===--------------------------------------------------------------------===//
  // Declaration Resolution Methods, for Decls with Headers and Bodies
  //===--------------------------------------------------------------------===//

  bool resolveHeader(Decl &D);
  bool resolveHeader(StructDecl &D);
  bool resolveHeader(EnumDecl &D);
  bool resolveHeader(FunDecl &D);
  bool resolveBodies(Decl &D);

  //===--------------------------------------------------------------------===//
  // Expression Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(Expr &E);
  bool visit(IntLiteral &E);
  bool visit(FloatLiteral &E);
  bool visit(StrLiteral &E);
  bool visit(CharLiteral &E);
  bool visit(BoolLiteral &E);
  bool visit(RangeLiteral &E);
  bool visit(TupleLiteral &E);
  bool visit(DeclRefExpr &E);
  bool visit(FunCallExpr &E);
  bool visit(BinaryOp &E);
  bool visit(UnaryOp &E);
  bool visit(CustomTypeCtor &E);
  bool visit(MemberInitExpr &E);
  bool visit(FieldAccessExpr &E);
  bool visit(MethodCallExpr &E);

  //===--------------------------------------------------------------------===//
  // Statement Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(Stmt &S);
  bool visit(ReturnStmt &S);
  bool visit(DeferStmt &S);
  bool visit(IfStmt &S);
  bool visit(WhileStmt &S);
  bool visit(ForStmt &S);
  bool visit(DeclStmt &S);
  bool visit(BreakStmt &S);
  bool visit(ContinueStmt &S);
  bool visit(ExprStmt &S);
  bool visit(Block &Block, bool ScopeCreated);

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  /// The AST being analyzed (function declarations)
  std::vector<std::unique_ptr<Decl>> Ast;
  SymbolTable SymbolTab;
  FunDecl *CurrentFun = nullptr;
  std::shared_ptr<DiagnosticManager> Diags;

  bool resolveStructCtor(StructDecl *Found, CustomTypeCtor &E);
  bool resolveEnumCtor(EnumDecl *Found, CustomTypeCtor &E);

  //===--------------------------------------------------------------------===//
  // Error Reporting Utilities
  //===--------------------------------------------------------------------===//

  void emitError(Diagnostic &Diagnostic) { Diags->emit(Diagnostic); }
  void emitRedefinitionError(std::string_view SymbolKind, Decl *FirstDecl,
                             Decl *Redecl);

  //===--------------------------------------------------------------------===//
  // Error Kind Classification
  //===--------------------------------------------------------------------===//

  enum class NotFoundErrorKind : uint8_t {
    Variable,
    Function,
    Type,
    Custom,
    Field,
    Variant,
  };

  //===--------------------------------------------------------------------===//
  // Generic Error Emission Template
  //===--------------------------------------------------------------------===//

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
    case NotFoundErrorKind::Custom:
      emitCustomTypeNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Field:
      emitFieldNotFound(PrimaryId, PrimaryLoc, ContextId);
      break;
    case NotFoundErrorKind::Variant:
      emitVariantNotFound(PrimaryId, PrimaryLoc, ContextId);
    }
  }

  //===--------------------------------------------------------------------===//
  // Specific Error Emission Methods
  //===--------------------------------------------------------------------===//

  void emitVariableNotFound(std::string_view VarId, const SrcLocation &Loc);
  void emitFunctionNotFound(std::string_view FunId, const SrcLocation &Loc);
  void emitTypeNotFound(std::string_view TypeName, const SrcLocation &Loc);
  void emitCustomTypeNotFound(std::string_view StructId,
                              const SrcLocation &Loc);
  void emitFieldNotFound(std::string_view FieldId, const SrcLocation &RefLoc,
                         const std::optional<std::string> &StructId);
  void emitVariantNotFound(std::string_view VariantId,
                           const SrcLocation &RefLoc,
                           const std::optional<std::string> &EnumId);
};

} // namespace phi
