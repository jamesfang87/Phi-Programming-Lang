#include "Sema/NameResolver.hpp"

#include <cassert>
#include <cstddef>

#include <llvm/Support/Casting.h>
#include <print>

#include "AST/Decl.hpp"
#include "AST/Expr.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "llvm/IR/CFG.h"

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
    return resolveStructCtor(Struct, E);
  }

  EnumDecl *Enum = SymbolTab.lookupEnum(E.getTypeName());
  if (Enum) {
    E.setDecl(Enum);
    return resolveEnumCtor(Enum, E);
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

    if (FieldInit->getInitValue()) {
      Success = visit(*FieldInit) && Success;
      continue;
    }

    if (FieldInit->getDecl()->hasInit()) {
      error("Field `{}` which already is initialized should not appear in "
            "constructor list unless field is to be initialized with something "
            "other than the default value.")
          .with_primary_label(FieldInit->getLocation(),
                              "Consider adding `= <value>` or removing this "
                              "field to solve this error")
          .emit(*Diags);
      Success = false;
    } else {
      error("Field `{}` cannot be uninitialized")
          .with_primary_label(FieldInit->getLocation(),
                              "Consider adding `= <value>` to solve this error")
          .emit(*Diags);
      Success = false;
    }
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

  bool Success = true;

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

  if (ActiveVariant.getInitValue()) {
    Success = visit(ActiveVariant) && Success;
  }

  if (VariantDecl) {
    E.setActiveVariant(VariantDecl);
    return true && Success;
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

/**
 * Resolves captured variables from a Variant pattern.
 * Creates VarDecl objects for each captured variable and adds them to scope.
 */
bool NameResolver::resolveVariantPattern(const PatternAtomics::Variant &P) {
  bool Success = true;
  for (const auto &CaptureDecl : P.Vars) {
    if (!SymbolTab.insert(CaptureDecl.get())) {
      emitRedefinitionError("variable", SymbolTab.lookup(*CaptureDecl),
                            CaptureDecl.get());
      Success = false;
    }
  }
  return Success;
}

/**
 * Resolves a singular pattern (used within alternations).
 */
bool NameResolver::resolveSingularPattern(
    const PatternAtomics::SingularPattern &Pat) {
  return std::visit(
      [this](const auto &P) -> bool {
        using T = std::decay_t<decltype(P)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          return true;
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          return visit(*P.Value);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          return resolveVariantPattern(P);
        }
      },
      Pat);
}

/**
 * Resolves a pattern, handling captured variables and literal expressions.
 */
bool NameResolver::resolvePattern(const Pattern &Pat) {
  return std::visit(
      [this](const auto &P) -> bool {
        using T = std::decay_t<decltype(P)>;
        if constexpr (std::is_same_v<T, PatternAtomics::Wildcard>) {
          return true;
        } else if constexpr (std::is_same_v<T, PatternAtomics::Literal>) {
          return visit(*P.Value);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Variant>) {
          return resolveVariantPattern(P);
        } else if constexpr (std::is_same_v<T, PatternAtomics::Alternation>) {
          bool Success = true;
          for (const auto &SubPattern : P.Patterns) {
            Success = resolveSingularPattern(SubPattern) && Success;
          }
          return Success;
        }
      },
      Pat);
}

/**
 * Resolves a match expression.
 *
 * Validates:
 * - Scrutinee resolves successfully
 * - All pattern literals resolve successfully
 * - Pattern captured variables are added to each arm's scope
 * - Each arm's body resolves in a scope containing captured variables
 */
bool NameResolver::visit(MatchExpr &E) {
  bool Success = visit(*E.getScrutinee());

  // Process each match arm
  for (auto &Arm : E.getArms()) {
    // Create a new scope for this arm to hold pattern captures
    SymbolTable::ScopeGuard ArmScope(SymbolTab);

    // Resolve pattern and add any captured variables to the scope
    Success = resolvePattern(Arm.Pattern) && Success;

    // Resolve arm body (scope already created)
    // The Return pointer is non-owning and points into the Body,
    // so we don't need to visit it separately
    Success = visit(*Arm.Body, true) && Success;
  }
  return Success;
}

} // namespace phi
