#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>
#include <variant>
#include <vector>

#include "AST/Nodes/Decl.hpp"
#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/Pattern.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/NameResolution/SymbolTable.hpp"
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

  NameResolver(std::vector<ModuleDecl *> Modules, DiagnosticManager *Diags)
      : Modules(std::move(Modules)), Diags(std::move(Diags)) {}

  //===--------------------------------------------------------------------===//
  // Main Entry Point
  //===--------------------------------------------------------------------===//

  std::vector<ModuleDecl *> resolve();
  ModuleDecl *resolveSingleMod(ModuleDecl *Module);

  //===--------------------------------------------------------------------===//
  // Type Visitor Method -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(TypeRef T);

  //===--------------------------------------------------------------------===//
  // Declaration Visitor Methods -> return bool (success/failure)
  //===--------------------------------------------------------------------===//

  bool visit(FunDecl *D);
  bool visit(ParamDecl *D);
  bool visit(StructDecl *D);
  bool visit(FieldDecl *D);
  bool visit(MethodDecl *D);
  bool visit(EnumDecl *D);
  bool visit(VariantDecl *D);

  //===--------------------------------------------------------------------===//
  // Declaration Resolution Methods, for Decls with Headers and Bodies
  //===--------------------------------------------------------------------===//

  bool resolveHeader(ItemDecl &D);
  bool resolveHeader(AdtDecl &D);
  bool resolveHeader(FunDecl &D);
  bool resolveHeader(MethodDecl &D);
  bool resolveBodies(ItemDecl &D);

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
  bool visit(AdtInit &E);
  bool resolveStructInit(StructDecl *Found, AdtInit &E);
  bool resolveEnumInit(EnumDecl *Found, AdtInit &E);
  bool visit(MemberInit &E);
  bool visit(FieldAccessExpr &E);
  bool visit(MethodCallExpr &E);
  bool visit(MatchExpr &E);
  bool visit(IntrinsicCall &E);

  //===--------------------------------------------------------------------===//
  // Pattern Resolution Methods
  //===--------------------------------------------------------------------===//

  bool resolvePattern(const std::vector<Pattern> &Pat);
  bool resolveSingularPattern(const Pattern &Pat);
  bool resolveVariantPattern(const PatternAtomics::Variant &P);

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

  std::vector<ModuleDecl *> Modules;
  SymbolTable SymbolTab;
  std::variant<FunDecl *, MethodDecl *, std::monostate> CurrentFun =
      std::monostate();
  DiagnosticManager *Diags;

  //===--------------------------------------------------------------------===//
  // Error Reporting Utilities
  //===--------------------------------------------------------------------===//

  void emitRedefinitionError(std::string_view SymbolKind, NamedDecl *FirstDecl,
                             NamedDecl *Redecl);

  //===--------------------------------------------------------------------===//
  // Error Kind Classification
  //===--------------------------------------------------------------------===//

  enum class NotFoundErrorKind : uint8_t {
    Variable,
    Function,
    Type,
    Adt,
    Field,
    Variant,
    ItemPath
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
    case NotFoundErrorKind::Adt:
      emitAdtNotFound(PrimaryId, PrimaryLoc);
      break;
    case NotFoundErrorKind::Field:
      emitFieldNotFound(PrimaryId, PrimaryLoc, ContextId);
      break;
    case NotFoundErrorKind::Variant:
      emitVariantNotFound(PrimaryId, PrimaryLoc, ContextId);
      break;
    case NotFoundErrorKind::ItemPath:
      break;
    }
  }

  //===--------------------------------------------------------------------===//
  // Specific Error Emission Methods
  //===--------------------------------------------------------------------===//

  void emitVariableNotFound(std::string_view VarId, const SrcLocation &Loc);
  void emitFunctionNotFound(std::string_view FunId, const SrcLocation &Loc);
  void emitTypeNotFound(std::string_view TypeName, const SrcLocation &Loc);
  void emitAdtNotFound(std::string_view StructId, const SrcLocation &Loc);
  void emitFieldNotFound(std::string_view FieldId, const SrcLocation &RefLoc,
                         const std::optional<std::string> &StructId);
  void emitVariantNotFound(std::string_view VariantId,
                           const SrcLocation &RefLoc,
                           const std::optional<std::string> &EnumId);
  void emitItemPathNotFound(std::string_view Path, const SrcSpan Span);
};

} // namespace phi
