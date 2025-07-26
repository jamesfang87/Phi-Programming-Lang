#include "Sema/Sema.hpp"

bool Sema::resolve_type(std::optional<Type> type) {
    if (!type.has_value()) {
        return false;
    }

    if (type.value().is_primitive()) {
        return true;
    }

    // TODO: support user-defined types
    return false;
}
