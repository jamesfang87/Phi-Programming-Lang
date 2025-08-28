#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/NameResolver.hpp"
#include <format>

namespace phi {

void NameResolver::emitRedefinitionError(std::string_view SymbolKind,
                                         Decl *FirstDecl, Decl *Redecl) {

  error(std::format("Redefinition of {} `{}`", SymbolKind, FirstDecl->getId()))
      .with_primary_label(Redecl->getLocation(), "Redeclaration here.")
      .with_code_snippet(FirstDecl->getLocation(), "First declared here:")
      .emit(*DiagnosticsMan);
}

} // namespace phi
