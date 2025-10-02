#include "Sema/NameResolver.hpp"

namespace phi {

bool NameResolver::resolveHeader(EnumDecl &D) { return true; }
bool NameResolver::visit(EnumDecl *D) { return true; }

bool NameResolver::visit(VariantDecl *D) { return true; }

} // namespace phi
