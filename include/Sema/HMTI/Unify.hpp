#pragma once
#include <algorithm>
#include <format>
#include <memory>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "Sema/HMTI/Substitution.hpp"
#include "Sema/HMTI/Types/Monotype.hpp"

namespace phi {

// ---------------------------
// Unification
// ---------------------------
struct UnifyError : std::runtime_error {
  using std::runtime_error::runtime_error;
};

inline bool occurs(const TypeVar &X, const Monotype &T) {
  if (T.isVar())
    return T.asVar() == X;
  return T.freeTypeVars().count(X) != 0;
}

inline Substitution bindVar(const TypeVar &X, const Monotype &T) {
  if (T.isVar() && T.asVar() == X)
    return {};
  if (occurs(X, T))
    throw UnifyError("occurs check failed: " + std::to_string(X.Id) + " in " +
                     T.toString());

  // Check constraints
  if (X.Constraints) {
    if (T.isCon()) {
      const std::string &name = T.asCon().Name;
      if (X.Constraints && !std::ranges::contains(*X.Constraints, name)) {
        std::string Msg =
            std::format("type constraint violation: found type {} cannot be "
                        "unified with expected types of:",
                        name);
        for (const auto &Possible : *X.Constraints) {
          Msg += Possible + ", ";
        }
        throw UnifyError(Msg);
      }
    } else if (T.isVar() && T.asVar().Constraints) {
      const TypeVar &Var = T.asVar();
      // Check if constraints are compatible
      std::vector<std::string> common;
      std::set_intersection(X.Constraints->begin(), X.Constraints->end(),
                            Var.Constraints->begin(), Var.Constraints->end(),
                            std::back_inserter(common));
      if (common.empty()) {
        throw UnifyError("incompatible type constraints");
      }
    }
  }

  Substitution s;
  s.Map.emplace(X, T);
  return s;
}

inline Substitution unify(const Monotype &a0, const Monotype &b0) {
  // Fast path: exact ptr-equal variant (shared object). Otherwise, structural.
  auto a = a0, b = b0;

  if (a.isVar())
    return bindVar(a.asVar(), b);
  if (b.isVar())
    return bindVar(b.asVar(), a);

  if (a.isCon() && b.isCon()) {
    const auto &ac = a.asCon(), &bc = b.asCon();
    if (ac.Name != bc.Name || ac.Args.size() != bc.Args.size())
      throw UnifyError("cannot unify " + a.toString() + " with " +
                       b.toString());
    Substitution s;
    for (size_t i = 0; i < ac.Args.size(); ++i) {
      auto si = unify(s.apply(ac.Args[i]), s.apply(bc.Args[i]));
      s.compose(si);
    }
    return s;
  }

  if (a.isFun() && b.isFun()) {
    const auto &af = a.asFun(), &bf = b.asFun();
    if (af.Params.size() != bf.Params.size())
      throw UnifyError("arity mismatch: " + a.toString() + " vs " +
                       b.toString());
    Substitution s;
    for (size_t i = 0; i < af.Params.size(); ++i) {
      auto si = unify(s.apply(af.Params[i]), s.apply(bf.Params[i]));
      s.compose(si);
    }
    auto sr = unify(s.apply(*af.Ret), s.apply(*bf.Ret));
    s.compose(sr);
    return s;
  }

  // Con vs Fun or any mismatch
  throw UnifyError("cannot unify " + a.toString() + " with " + b.toString());
}

} // namespace phi
