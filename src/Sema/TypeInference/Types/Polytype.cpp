#include "Sema/TypeInference/Types/Polytype.hpp"

#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"
#include <unordered_set>

namespace phi {

std::unordered_set<TypeVar> Polytype::freeTypeVars() const {
  auto FreeTypeVars = Body.freeTypeVars();
  for (auto &Q : Quant)
    FreeTypeVars.erase(Q);
  return FreeTypeVars;
}

Monotype Polytype::instantiate(TypeVarFactory &Factory) {
  if (this->getQuant().empty())
    return this->getBody();
  // Make fresh vars for each.getQuant() and substitute
  Substitution Subst;
  for (auto &Q : this->getQuant()) {
    // Preserve constraints when creating fresh variables
    Monotype FreshVar = Q.Constraints ?
      Monotype::makeVar(Factory.fresh(), *Q.Constraints) :
      Monotype::makeVar(Factory.fresh());
    Subst.Map.emplace(Q, FreshVar);
  }
  return Subst.apply(this->getBody());
}

} // namespace phi
