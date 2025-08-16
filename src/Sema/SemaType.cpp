#include "Sema/Sema.hpp"
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
bool Sema::resolveTy(std::optional<Type> Type) {
  if (!Type.has_value()) {
    return false;
  }

  if (Type.value().isPrimitive()) {
    return true;
  }

  std::string TypeName = Type.value().getCustomTypeName();
  if (SymbolTab.lookup(TypeName)) {
    return true;
  }

  return false;
}

} // namespace phi
