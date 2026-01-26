#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <llvm-18/llvm/ADT/StringRef.h>
#include <llvm/Support/Casting.h>
#include <memory>

#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"

namespace phi {

std::vector<ModuleDecl *> NameResolver::resolve() {
  // we first fill a table of all importable items
  // and then we try to resolve each module
  for (auto &Mod : Modules) {
    SymbolTab.insertAsImportable(Mod);

    for (auto *Item : Mod->getPublicItems()) {
      SymbolTab.insertAsImportable(Item, Mod);
    }
  }

  for (auto &Mod : Modules) {
    Mod = resolveSingleMod(std::move(Mod));
  }

  return std::move(Modules);
}

ModuleDecl *NameResolver::resolveSingleMod(ModuleDecl *Module) {
  SymbolTable::ScopeGuard ModuleScope(SymbolTab);

  // Resolve the imports into the symbol table;
  // make sure we aren't trying to import something inside this module

  // Decl with bodies are handled in 2 phases.
  // Phase 1: Resolve signatures
  // Phase 2: Resolve bodies

  for (auto &Import : Module->getImports()) {
    auto *Decl = SymbolTab.lookupImport(Import.getPathStr());
    if (!Decl) {
      emitItemPathNotFound(Import.getPathStr(), Import.getSpan());
      continue;
    }

    Import.setImportedDecl(Decl);
    if (auto *Mod = llvm::dyn_cast<ModuleDecl>(Import.getImportedDecl())) {
      if (Mod == Module) {
        error(
            std::format("cannot import module `{}` from itself", Mod->getId()))
            .with_primary_label(Mod->getSpan())
            .emit(*Diags);
        continue;
      }

      for (auto *Item : Mod->getPublicItems()) {
        SymbolTab.insertWithQual(Item, Mod->getPath().back());
      }

      continue;
    }

    if (auto *Item = llvm::dyn_cast<ItemDecl>(Import.getImportedDecl())) {
      assert(!llvm::isa<ModuleDecl>(Item));
      if (Module->contains(Item)) {
        error(std::format(
                  "cannot import item `{}` from the module which contains it",
                  Item->getId()))
            .with_primary_label(Item->getSpan(), "remove this import")
            .emit(*Diags);
      }
      SymbolTab.insert(Item);
    }
  }

  for (auto &Decl : Module->getItems()) {
    resolveHeader(*Decl);
  }

  // Phase 2: Resolve function bodies
  for (auto &Decl : Module->getItems()) {
    resolveBodies(*Decl);
  }

  return Module;
}

} // namespace phi
