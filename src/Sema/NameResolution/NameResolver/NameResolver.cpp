#include "Sema/NameResolution/NameResolver.hpp"

#include <cassert>
#include <llvm/ADT/StringRef.h>
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
        auto Alias = Import.getAlias().value_or(Mod->getPath().back());
        if (!SymbolTab.insertWithQual(Item, Alias)) {
          error(std::format("redefinition of `{}`", Alias))
              .with_primary_label(Import.getSpan())
              .emit(*Diags);
        }
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

      auto Alias = Import.getAlias().value_or(Item->getId());
      if (!SymbolTab.insertAs(Item, Alias)) {
        error(std::format("redefinition of `{}`", Alias))
            .with_primary_label(Import.getSpan())
            .emit(*Diags);
      }
    }
  }

  for (auto &Use : Module->getUses()) {
    static const std::unordered_map<std::string_view, BuiltinTy::Kind>
        PrimitiveMap = {
            {"i8", BuiltinTy::i8},         {"i16", BuiltinTy::i16},
            {"i32", BuiltinTy::i32},       {"i64", BuiltinTy::i64},
            {"u8", BuiltinTy::u8},         {"u16", BuiltinTy::u16},
            {"u32", BuiltinTy::u32},       {"u64", BuiltinTy::u64},
            {"f32", BuiltinTy::f32},       {"f64", BuiltinTy::f64},
            {"string", BuiltinTy::String}, {"char", BuiltinTy::Char},
            {"bool", BuiltinTy::Bool},
        };

    if (PrimitiveMap.contains(Use.getPathStr())) {
      if (auto *D = SymbolTab.lookupImport(Use.getAlias())) {
        error(
            std::format("Naming conflict with type alias `{}`", Use.getAlias()))
            .with_extra_snippet(D->getSpan(), "with this declaration here")
            .emit(*Diags);
      }
      continue;
    }

    auto *Decl = SymbolTab.lookupImport(Use.getPathStr());
    if (!Decl) {
      emitItemPathNotFound(Use.getPathStr(), Use.getSpan());
      continue;
    }

    Use.setAliasedDecl(Decl);
    if (auto *Mod = llvm::dyn_cast<ModuleDecl>(Use.getAliasedDecl())) {
      for (auto *Item : Mod->getPublicItems()) {
        if (!SymbolTab.insertWithQual(Item, Use.getAlias())) {
          error(std::format("redefinition of `{}`", Use.getAlias()))
              .with_primary_label(Use.getSpan())
              .emit(*Diags);
        }
      }
      continue;
    }

    if (auto *Item = llvm::dyn_cast<ItemDecl>(Use.getAliasedDecl())) {
      assert(!llvm::isa<ModuleDecl>(Item));
      if (!SymbolTab.insertAs(Item, Use.getAlias())) {
        error(std::format("redefinition of `{}`", Use.getAlias()))
            .with_primary_label(Use.getSpan())
            .emit(*Diags);
      }
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
