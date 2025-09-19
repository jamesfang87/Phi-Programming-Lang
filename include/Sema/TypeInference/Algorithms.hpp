#pragma once

#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"
#include "Sema/TypeInference/TypeVarFactory.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

// ---------------------------
// Instantiate / Generalize
// ---------------------------
inline Monotype instantiate(const Polytype &P, TypeVarFactory &Factory) {
  if (P.getQuant().empty())
    return P.getBody();
  // Make fresh vars for each.getQuant() and substitute
  Substitution Subst;
  for (auto &Q : P.getQuant()) {
    Subst.Map.emplace(Q, Monotype::makeVar(Factory.fresh()));
  }
  return Subst.apply(P.getBody());
}

inline Polytype generalize(const TypeEnv &Env, const Monotype &T) {
  const auto MonotypeFTVs = T.freeTypeVars();
  const auto EnvFTVs = Env.freeTypeVars();

  // Quant = ftv(t) \ ftv(env)
  std::vector<TypeVar> Quant;
  for (auto &TypeVar : MonotypeFTVs)
    if (!EnvFTVs.contains(TypeVar))
      Quant.push_back(TypeVar);

  return {std::move(Quant), T};
}

} // namespace phi
