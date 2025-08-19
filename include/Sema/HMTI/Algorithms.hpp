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
inline Monotype instantiate(const Polytype &s, TypeVarFactory &Factory) {
  if (s.getQuant().empty())
    return s.getBody();
  // Make fresh vars for each.getQuant() and substitute
  Substitution sub;
  for (auto &q : s.getQuant()) {
    sub.Map.emplace(q, Monotype::makeVar(Factory.fresh()));
  }
  return sub.apply(s.getBody());
}

inline Polytype generalize(const TypeEnv &Env, const Monotype &t) {
  auto tvs = t.freeTypeVars();
  auto envtvs = Env.freeTypeVars();
  // quant = ftv(t) \ ftv(env)
  std::vector<TypeVar> quant;
  for (auto &v : tvs)
    if (!envtvs.count(v))
      quant.push_back(v);

  return {std::move(quant), t};
}

} // namespace phi
