#include "Sema/Sema.hpp"

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
bool Sema::resolveTy(std::optional<Type> type) {
  if (!type.has_value()) {
    return false;
  }

  if (type.value().isPrimitive()) {
    return true;
  }

  // TODO: support user-defined types
  return false;
}

} // namespace phi
