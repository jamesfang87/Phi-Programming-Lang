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
  for (auto &q : P.getQuant()) {
    Subst.Map.emplace(q, Monotype::makeVar(Factory.fresh()));
  }
  return Subst.apply(P.getBody());
}

inline Polytype generalize(const TypeEnv &Env, const Monotype &t) {
  const auto MonotypeFTVs = t.freeTypeVars();
  const auto EnvFTVs = Env.freeTypeVars();

  // Quant = ftv(t) \ ftv(env)
  std::vector<TypeVar> Quant;
  for (auto &v : MonotypeFTVs)
    if (!EnvFTVs.contains(v))
      Quant.push_back(v);

  return {std::move(Quant), t};
}

} // namespace phi
