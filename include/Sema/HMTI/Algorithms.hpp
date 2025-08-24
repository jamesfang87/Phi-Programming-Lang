#pragma once

#include "Sema/HMTI/Substitution.hpp"
#include "Sema/HMTI/TypeEnv.hpp"
#include "Sema/HMTI/TypeVarFactory.hpp"
#include "Sema/HMTI/Types/Monotype.hpp"
#include "Sema/HMTI/Types/Polytype.hpp"

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
  auto MonotypeFTVs = t.freeTypeVars();
  auto EnvFTVs = Env.freeTypeVars();

  // Quant = ftv(t) \ ftv(env)
  std::vector<TypeVar> Quant;
  for (auto &v : MonotypeFTVs)
    if (!EnvFTVs.count(v))
      Quant.push_back(v);

  return {std::move(Quant), t};
}

} // namespace phi
