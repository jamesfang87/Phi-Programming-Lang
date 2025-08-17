#pragma once

#include "AST/Type.hpp"
#include "Sema/HMTI/HMType.hpp"

namespace phi {

// Convert AST Type -> Monotype
std::shared_ptr<Monotype> fromAstType(const Type &T);

// Convert Monotype -> AST Type (used when annotating)
Type toAstType(const std::shared_ptr<Monotype> &T);

} // namespace phi
