#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/NameResolver.hpp"
#include <format>
#include <string>

namespace phi {

void NameResolver::emitRedefinitionError(std::string_view SymbolKind,
                                         Decl *FirstDecl, Decl *Redecl) {

  error(std::format("Redefinition of {} `{}`", SymbolKind, FirstDecl->getId()))
      .with_primary_label(Redecl->getLocation(), "Redeclaration here.")
      .with_code_snippet(FirstDecl->getLocation(), "First declared here:")
      .emit(*DiagnosticsMan);
}

void NameResolver::emitVariableNotFound(std::string_view VarId,
                                        const SrcLocation &Loc) {

  auto BestMatch = SymbolTab.getClosestVar(std::string(VarId));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  auto PrimaryMsg =
      std::format("Declaration for `{}` could not be found. {}", VarId, Hint);
  error(std::format("use of undeclared variable `{}`", VarId))
      .with_primary_label(Loc, PrimaryMsg)
      .emit(*DiagnosticsMan);
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
  error(std::format(
            "Could not match type `{}` with any primitive type or a struct",
            TypeName))
      .with_primary_label(Loc, PrimaryMsg)
      .emit(*DiagnosticsMan);
}

void NameResolver::emitStructNotFound(std::string_view StructId,
                                      const SrcLocation &Loc) {
  auto BestMatch = SymbolTab.getClosestStruct(std::string(StructId));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  auto PrimaryMsg = std::format("No declaration for struct `{}` was found. {}",
                                StructId, Hint);
  auto Diag = error(std::format("Could not find struct `{}`", StructId))
                  .with_primary_label(Loc, PrimaryMsg);

  Diag.emit(*DiagnosticsMan);
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

  Diag.emit(*DiagnosticsMan);
}

void NameResolver::emitFunctionNotFound(std::string_view FunId,
                                        const SrcLocation &Loc) {
  auto BestMatch = SymbolTab.getClosestFun(std::string(FunId));
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", BestMatch->getId());
  }

  error(std::format("attempt to call undeclared function `{}`", FunId))
      .with_primary_label(
          Loc, std::format("Declaration for `{}` could not be found. {}", FunId,
                           Hint))
      .emit(*DiagnosticsMan);
}

} // namespace phi
