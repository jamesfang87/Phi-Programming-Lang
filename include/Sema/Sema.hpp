#include "AST/Nodes/Decl.hpp"
#include "Diagnostics/DiagnosticManager.hpp"
#include "Sema/NameResolution/NameResolver.hpp"
#include "Sema/TypeInference/Inferencer.hpp"

namespace phi {

class Sema {
public:
  Sema(std::vector<ModuleDecl *> Mods, DiagnosticManager *Diags)
      : Mods(std::move(Mods)), Diags(Diags) {}

  bool analyze() {
    auto Resolved = NameResolver(Mods, Diags).resolve();
    if (Diags->hasError()) {
      return false;
    }

    auto Checked = TypeInferencer(Resolved, Diags).infer();
    if (Diags->hasError()) {
      return false;
    }

    for (auto &Mod : Checked) {
      Mod->emit(0);
    }
    return true;
  }

private:
  std::vector<ModuleDecl *> Mods;
  DiagnosticManager *Diags;
};

} // namespace phi
