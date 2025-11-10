#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/NameResolver.hpp"

#include <cassert>
#include <cstddef>

#include <llvm/Support/Casting.h>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"

namespace phi {

bool NameResolver::visit(Expr &E) { return E.accept(*this); }

// Literal expression resolution
// trivial: we don't need any name resolution
bool NameResolver::visit(IntLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(FloatLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(StrLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(CharLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(BoolLiteral &E) {
  (void)E;
  return true;
}

bool NameResolver::visit(RangeLiteral &E) {
  // Resolve start and end expressions
  return visit(E.getStart()) && visit(E.getEnd());
}

bool NameResolver::visit(TupleLiteral &E) {
  bool Success = true;
  for (auto &Element : E.getElements()) {
    Success = visit(*Element) && Success;
  }
  return Success;
}

/**
 * Resolves a variable reference expression.
 *
 * Validates:
 * - Identifier exists in symbol table
 * - Not attempting to use function as value
 */
bool NameResolver::visit(DeclRefExpr &E) {
  ValueDecl *DeclPtr = SymbolTab.lookup(E);

  if (!DeclPtr) {
    emitNotFoundError(NotFoundErrorKind::Variable, E.getId(), E.getLocation());
    return false;
  }

  if (llvm::isa<FieldDecl>(DeclPtr)) {
    auto PrimaryMsg = std::format("Declaration for `{}` could not be found.",
                                  DeclPtr->getId());
    error(std::format("use of undeclared variable `{}`", DeclPtr->getId()))
        .with_primary_label(E.getLocation(), PrimaryMsg)
        .with_note(std::format("If you meant to access the field `{}`, please "
                               "prefix this with `self.` as in `self.{}` ",
                               DeclPtr->getId(), DeclPtr->getId()))
        .emit(*Diags);
  }

  E.setDecl(DeclPtr);
  return true;
}

/**
 * Resolves a function call expression.
 */
bool NameResolver::visit(FunCallExpr &E) {
  bool Success = true;
  FunDecl *FunPtr = SymbolTab.lookup(E);
  if (!FunPtr) {
    auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
    emitNotFoundError(NotFoundErrorKind::Function, DeclRef->getId(),
                      E.getLocation());
    Success = false;
  }

  for (auto &Arg : E.getArgs()) {
    Success = visit(*Arg) && Success;
  }

  E.setDecl(FunPtr);
  return Success;
}

/**
 * Resolves a binary operation expression.
 *
 * Validates:
 * - Both operands resolve successfully
 * - Operation is supported for operand types
 * - Type promotion rules are followed
 */
bool NameResolver::visit(BinaryOp &E) {
  // Resolve operands
  return visit(E.getLhs()) && visit(E.getRhs());
}

/**
 * Resolves a unary operation expression.
 *
 * Validates:
 * - Operand resolves successfully
 * - Operation is supported for operand type
 */
bool NameResolver::visit(UnaryOp &E) { return visit(E.getOperand()); }

bool NameResolver::visit(CustomTypeCtor &E) {
  if (E.isAnonymous()) {
    // we must delegate name resolution to after the type is known
    return true;
  }

  StructDecl *Struct = SymbolTab.lookupStruct(E.getTypeName());
  if (Struct) {
    E.setDecl(Struct);
    resolveStructCtor(Struct, E);
  }

  EnumDecl *Enum = SymbolTab.lookupEnum(E.getTypeName());
  if (Enum) {
    E.setDecl(Enum);
    resolveEnumCtor(Enum, E);
  }

  emitNotFoundError(NotFoundErrorKind::Custom, E.getTypeName(),
                    E.getLocation());
  return false;
}

bool NameResolver::resolveStructCtor(StructDecl *Found, CustomTypeCtor &E) {
  assert(Found != nullptr);

  std::unordered_set<std::string> Missing;
  for (auto &Field : Found->getFields()) {
    if (!Field->hasInit())
      Missing.insert(Field->getId());
  }

  bool Success = true;
  for (auto &FieldInit : E.getInits()) {
    if (Found->getField(FieldInit->getId()) == nullptr) {
      emitNotFoundError(NotFoundErrorKind::Field, FieldInit->getId(),
                        FieldInit->getLocation());
    } else {
      FieldInit->setDecl(Found->getField(FieldInit->getId()));
      assert(FieldInit->getDecl() != nullptr);
      Missing.erase(FieldInit->getId());
    }

    Success = visit(*FieldInit) && Success;
  }

  if (Missing.empty()) {
    return Success;
  }

  std::string Err = "Struct " + Found->getId() + "is missing inits for fields ";
  for (const std::string &Field : Missing) {
    Err += Field + ", ";
  }
  error(Err.substr(0, Err.size() - 2))
      .with_primary_label(E.getLocation(), "For this init")
      .emit(*Diags);

  return false;
}

bool NameResolver::resolveEnumCtor(EnumDecl *Found, CustomTypeCtor &E) {
  assert(Found != nullptr);
  assert(E.getInterpretation() == CustomTypeCtor::InterpretAs::Enum);
  assert(std::holds_alternative<EnumDecl *>(E.getDecl()));
  assert(std::get<EnumDecl *>(E.getDecl()) != nullptr);

  // 1. Check that we only specify 1 active variant
  if (E.getInits().size() != 1) {
    error("Enums can only hold exactly 1 active variant")
        .with_primary_label(E.getLocation(), "For this init")
        .emit(*Diags);
    return false;
  }

  // 2. Check that the init is actually a variant of the enum
  assert(E.getInits().size() == 1);
  auto &ActiveVariant = *E.getInits().front();
  auto *VariantDecl = Found->getVariant(ActiveVariant.getId());
  if (VariantDecl) {
    E.setActiveVariant(VariantDecl);
    return true;
  }

  emitNotFoundError(NotFoundErrorKind::Variant, ActiveVariant.getId(),
                    E.getLocation(), E.getTypeName());

  return false;
}

bool NameResolver::visit(MemberInitExpr &E) {
  assert(E.getInitValue() != nullptr);

  return visit(*E.getInitValue());
}

bool NameResolver::visit(FieldAccessExpr &E) { return visit(*E.getBase()); }

bool NameResolver::visit(MethodCallExpr &E) {
  bool Success = visit(*E.getBase());

  for (const auto &Args : E.getArgs()) {
    Success = visit(*Args) && Success;
  }

  return Success;
}

} // namespace phi
