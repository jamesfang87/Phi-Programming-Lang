#include "Sema/Sema.hpp"

std::optional<Type> Sema::resolve_type(Type type) {
    if (type.is_primitive()) {
        return type;
    }

    // TODO: support user-defined types
    return std::nullopt;
}
