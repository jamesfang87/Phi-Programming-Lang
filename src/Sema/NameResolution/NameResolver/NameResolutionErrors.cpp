#include "AST/Nodes/Decl.hpp"
#include "Sema/NameResolution/NameResolver.hpp"

#include <format>
#include <string>

#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

void NameResolver::emitRedefinitionError(std::string_view SymbolKind,
                                         NamedDecl *FirstDecl,
                                         NamedDecl *Redecl) {

  error(std::format("Redefinition of {} `{}`", SymbolKind, FirstDecl->getId()))
      .with_primary_label(Redecl->getSpan(), "Redeclaration here.")
      .with_code_snippet(FirstDecl->getSpan(), "First declared here:")
      .emit(*Diags);
}

void NameResolver::emitVariableNotFound(std::string_view VarId,
                                        const SrcLocation &Loc) {

  auto *BestMatch = SymbolTab.getClosestLocal(std::string(VarId));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  auto PrimaryMsg =
      std::format("Declaration for `{}` could not be found. {}", VarId, Hint);
  error(std::format("use of undeclared variable `{}`", VarId))
      .with_primary_label(Loc, PrimaryMsg)
      .emit(*Diags);
}

void NameResolver::emitTypeNotFound(std::string_view TypeName,
                                    const SrcLocation &Loc) {
  auto BestMatch = SymbolTab.getClosestType(std::string(TypeName));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", *BestMatch);
  }

  auto PrimaryMsg =
      std::format("Expected `{}` to be a valid type. {}", TypeName, Hint);
  error(
      std::format("Could not match type `{}` with any primitive or custom type",
                  TypeName))
      .with_primary_label(Loc, PrimaryMsg)
      .emit(*Diags);
}

void NameResolver::emitAdtNotFound(std::string_view Id,
                                   const SrcLocation &Loc) {
  auto *BestMatch = SymbolTab.getClosestAdt(std::string(Id));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  auto Msg = std::format("No declaration for id `{}` was found. {}", Id, Hint);
  auto Diag = error(std::format("Could not find id `{}`", Id))
                  .with_primary_label(Loc, Msg);

  Diag.emit(*Diags);
}

void NameResolver::emitFieldNotFound(
    std::string_view FieldId, const SrcLocation &RefLoc,
    const std::optional<std::string> &StructId) {
  std::string PrimaryMsg;
  if (StructId) {
    PrimaryMsg = std::format("Struct `{}` does not declare a field named `{}`.",
                             *StructId, FieldId);
  } else {
    PrimaryMsg = std::format("No field named `{}` found.", FieldId);
  }

  auto Diag = error(std::format("Could not find field `{}`", FieldId))
                  .with_primary_label(RefLoc, PrimaryMsg);

  Diag.emit(*Diags);
}

void NameResolver::emitVariantNotFound(
    std::string_view VariantId, const SrcLocation &RefLoc,
    const std::optional<std::string> &EnumId) {
  std::string PrimaryMsg;
  if (EnumId) {
    PrimaryMsg = std::format("Enum `{}` does not declare a variant named `{}`.",
                             *EnumId, VariantId);
  } else {
    PrimaryMsg = std::format("No variant named `{}` found.", VariantId);
  }

  auto Diag = error(std::format("Could not find variant `{}`", VariantId))
                  .with_primary_label(RefLoc, PrimaryMsg);

  Diag.emit(*Diags);
}

void NameResolver::emitFunctionNotFound(std::string_view FunId,
                                        const SrcLocation &Loc) {
  auto *BestMatch = SymbolTab.getClosestFun(std::string(FunId));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  error(std::format("attempt to call undeclared function `{}`", FunId))
      .with_primary_label(
          Loc, std::format("Declaration for `{}` could not be found. {}", FunId,
                           Hint))
      .emit(*Diags);
}

void NameResolver::emitItemPathNotFound(std::string_view Path,
                                        const SrcSpan Span) {
  error(std::format("could not find module or item `{}` to import", Path))
      .with_primary_label(
          Span, std::format("Declaration for `{}` could not be found", Path))
      .emit(*Diags);
}

} // namespace phi
