#include "AST/Type.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/NameResolver.hpp"
#include <format>
#include <string>

namespace phi {

/**
 * Resolves a type specification to ensure it's valid.
 *
 * @param type The type to resolve (optional)
 * @return true if type is valid, false otherwise
 *
 * Current capabilities:
 * - Always validates primitive types
 * - Placeholder for future user-defined type support
 * - Returns false for unresolved types
 */
bool NameResolver::resolveType(std::optional<Type> Type) {
  if (!Type.has_value()) {
    return false;
  }

  class Type T = Type.value();
  if (T.isPointer()) {
    T = *T.asPtr().Pointee;
  }

  if (T.isReference()) {
    T = *T.asRef().Pointee;
  }

  if (T.isPrimitive()) {
    return true;
  }

  std::string TypeName = T.toString();
  if (SymbolTab.lookup(TypeName)) {
    return true;
  }

  auto BestMatch = SymbolTab.getClosestType(TypeName);
  std::string Hint;
  if (BestMatch) {
    Hint = std::format("Did you mean `{}`?", *BestMatch);
  }

  error(std::format(
            "Could not match type `{}` with any primitive type or a struct",
            TypeName))
      .with_primary_label(
          T.getLocation(),
          std::format("Expected this to be a valid type. {}", Hint))
      .emit(*DiagnosticsMan);
  return false;
}

} // namespace phi
