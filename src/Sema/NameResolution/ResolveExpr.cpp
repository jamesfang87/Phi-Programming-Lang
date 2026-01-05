#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <unordered_set>

#include <llvm/ADT/TypeSwitch.h>
#include <llvm/Support/Casting.h>

#include "AST/Nodes/Expr.hpp"
#include "AST/Nodes/Stmt.hpp"
#include "AST/TypeSystem/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

bool NameResolver::visit(Expr &E) {
  return llvm::TypeSwitch<Expr *, bool>(&E)
      .Case<IntLiteral>([&](IntLiteral *X) { return visit(*X); })
      .Case<FloatLiteral>([&](FloatLiteral *X) { return visit(*X); })
      .Case<StrLiteral>([&](StrLiteral *X) { return visit(*X); })
      .Case<CharLiteral>([&](CharLiteral *X) { return visit(*X); })
      .Case<BoolLiteral>([&](BoolLiteral *X) { return visit(*X); })
      .Case<RangeLiteral>([&](RangeLiteral *X) { return visit(*X); })
      .Case<TupleLiteral>([&](TupleLiteral *X) { return visit(*X); })
      .Case<DeclRefExpr>([&](DeclRefExpr *X) { return visit(*X); })
      .Case<FunCallExpr>([&](FunCallExpr *X) { return visit(*X); })
      .Case<BinaryOp>([&](BinaryOp *X) { return visit(*X); })
      .Case<UnaryOp>([&](UnaryOp *X) { return visit(*X); })
      .Case<MemberInit>([&](MemberInit *X) { return visit(*X); })
      .Case<FieldAccessExpr>([&](FieldAccessExpr *X) { return visit(*X); })
      .Case<MethodCallExpr>([&](MethodCallExpr *X) { return visit(*X); })
      .Case<MatchExpr>([&](MatchExpr *X) { return visit(*X); })
      .Case<AdtInit>([&](AdtInit *X) { return visit(*X); })
      .Default([&](Expr *) {
        llvm_unreachable("Unhandled Expr kind in TypeInferencer");
        return false;
      });
}

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
  ValueDecl *Decl = SymbolTab.lookup(E);

  if (!Decl) {
    emitNotFoundError(NotFoundErrorKind::Variable, E.getId(), E.getLocation());
    return false;
  }

  if (llvm::isa<FieldDecl>(Decl)) {
    auto PrimaryMsg =
        std::format("Declaration for `{}` could not be found.", Decl->getId());
    error(std::format("use of undeclared variable `{}`", Decl->getId()))
        .with_primary_label(E.getLocation(), PrimaryMsg)
        .with_note(std::format("If you meant to access the field `{}`, please "
                               "prefix this with `self.` as in `self.{}` ",
                               Decl->getId(), Decl->getId()))
        .emit(*Diags);
  }

  E.setDecl(Decl);
  return true;
}

/**
 * Resolves a function call expression.
 */
bool NameResolver::visit(FunCallExpr &E) {
  bool Success = true;
  FunDecl *Decl = SymbolTab.lookup(E);
  if (!Decl) {
    auto *DeclRef = llvm::dyn_cast<DeclRefExpr>(&E.getCallee());
    emitNotFoundError(NotFoundErrorKind::Function, DeclRef->getId(),
                      E.getLocation());
    Success = false;
  }

  for (auto &Arg : E.getArgs()) {
    Success = visit(*Arg) && Success;
  }

  E.setDecl(Decl);
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

bool NameResolver::visit(AdtInit &E) {
  if (E.isAnonymous()) {
    // we must delegate name resolution to after the type is known
    return true;
  }

  auto *Decl = SymbolTab.lookup(E.getTypeName());
  if (!Decl) {
    emitNotFoundError(NotFoundErrorKind::Adt, E.getTypeName(), E.getLocation());
    return false;
  }

  E.setDecl(Decl);
  llvm::dyn_cast<AdtTy>(E.getType().getPtr())->setDecl(Decl);
  return llvm::TypeSwitch<AdtDecl *, bool>(SymbolTab.lookup(E.getTypeName()))
      .Case<StructDecl>([&](auto *D) { return resolveStructCtor(D, E); })
      .Case<EnumDecl>([&](auto *D) { return resolveEnumCtor(D, E); });
}

bool NameResolver::resolveStructCtor(StructDecl *Found, AdtInit &E) {
  assert(Found != nullptr);

  std::unordered_set<std::string> Missing;
  for (auto &Field : Found->getFields()) {
    if (!Field.hasInit())
      Missing.insert(Field.getId());
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

bool NameResolver::resolveEnumCtor(EnumDecl *Found, AdtInit &E) {
  assert(Found != nullptr);

  bool Success = true;

  // 1. Check that we only specify 1 active variant
  if (E.getInits().size() != 1) {
    error("Enums can only hold exactly 1 active variant")
        .with_primary_label(E.getLocation(), "For this init")
        .emit(*Diags);
    return false;
  }

  // 2. Check that the specfied variant is actually a variant of the enum
  assert(E.getInits().size() == 1);
  auto &ActiveVariant = *E.getInits().front();
  auto *VariantDecl = Found->getVariant(ActiveVariant.getId());
  if (!VariantDecl) {
    emitNotFoundError(NotFoundErrorKind::Variant, ActiveVariant.getId(),
                      E.getLocation(), E.getTypeName());
    return false;
  }
  E.setActiveVariant(VariantDecl);

  // 3. Check that if the Variant has no payload, we don't give a payload
  if (ActiveVariant.getInitValue()) {
    Success = visit(ActiveVariant) && Success;
  }

  if (VariantDecl->hasType() == (ActiveVariant.getInitValue() != nullptr)) {
    return Success;
  }

  if (VariantDecl->hasType()) {
    error(std::format(
              "No payload given for variant `{}`, which requires a payload",
              VariantDecl->getId()))
        .with_primary_label(ActiveVariant.getSpan(), "Add a payload here")
        .with_extra_snippet(VariantDecl->getSpan(), "Variant declared here")
        .emit(*Diags);
  } else {
    error(std::format("Payload given for variant `{}`, which has no payload",
                      VariantDecl->getId()))
        .with_primary_label(ActiveVariant.getSpan(), "remove this payload")
        .with_extra_snippet(VariantDecl->getSpan(), "Variant declared here")
        .emit(*Diags);
  }

  return false;
}

bool NameResolver::visit(MemberInit &E) {
  if (!E.getInitValue()) {
    return true;
  }

  return visit(*E.getInitValue());
}

bool NameResolver::visit(FieldAccessExpr &E) {
  // We can know with certainty the Field which E is accessing at this point.
  // Thus, we defer name resolution to after type inference,
  // where upon we can know the type.
  return visit(*E.getBase());
}

bool NameResolver::visit(MethodCallExpr &E) {
  // We can know with certainty the Method which E is calling at this point.
  // Thus, we defer name resolution to after type inference,
  // where upon we can know the type.
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
bool NameResolver::resolveSingularPattern(const Pattern &Pat) {
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
bool NameResolver::resolvePattern(const std::vector<Pattern> &Patterns) {
  bool Success = true;
  for (const auto &SubPattern : Patterns) {
    Success = resolveSingularPattern(SubPattern) && Success;
  }
  return Success;
}

bool NameResolver::visit(MatchExpr &E) {
  bool Success = visit(*E.getScrutinee());

  // Process each match arm
  for (auto &Arm : E.getArms()) {
    // Create a new scope for this arm to hold pattern captures
    SymbolTable::ScopeGuard ArmScope(SymbolTab);

    // Resolve pattern and add any captured variables to the scope
    Success = resolvePattern(Arm.Patterns) && Success;

    // Resolve arm body (scope already created)
    // The Return pointer is non-owning and points into the Body,
    // so we don't need to visit it separately
    Success = visit(*Arm.Body, true) && Success;
  }
  return Success;
}

} // namespace phi
