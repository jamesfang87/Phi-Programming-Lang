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

inline bool occurs(const TypeVar &x, const Monotype &t) {
  if (t.isVar())
    return t.asVar() == x;
  return t.freeTypeVars().count(x) != 0;
}

inline Substitution bindVar(const TypeVar &v, const Monotype &t) {
  if (t.isVar() && t.asVar() == v)
    return {};
  if (occurs(v, t))
    throw UnifyError("occurs check failed: " + std::to_string(v.Id) + " in " +
                     t.toString());

  // Check constraints
  if (v.Constraints) {
    if (t.isCon()) {
      const std::string &name = t.asCon().name;
      if (v.Constraints && !std::ranges::contains(*v.Constraints, name)) {
        std::string Msg =
            std::format("type constraint violation: found type {} cannot be "
                        "unified with expected types of:",
                        name);
        for (const auto &Possible : *v.Constraints) {
          Msg += Possible + ", ";
        }
        throw UnifyError(Msg);
      }
    } else if (t.isVar() && t.asVar().Constraints) {
      const TypeVar &Var = t.asVar();
      // Check if constraints are compatible
      std::vector<std::string> common;
      std::set_intersection(v.Constraints->begin(), v.Constraints->end(),
                            Var.Constraints->begin(), Var.Constraints->end(),
                            std::back_inserter(common));
      if (common.empty()) {
        throw UnifyError("incompatible type constraints");
      }
    }
  }

  Substitution s;
  s.Map.emplace(v, t);
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
    if (ac.name != bc.name || ac.args.size() != bc.args.size())
      throw UnifyError("cannot unify " + a.toString() + " with " +
                       b.toString());
    Substitution s;
    for (size_t i = 0; i < ac.args.size(); ++i) {
      auto si = unify(s.apply(ac.args[i]), s.apply(bc.args[i]));
      s.compose(si);
    }
    return s;
  }

  if (a.isFun() && b.isFun()) {
    const auto &af = a.asFun(), &bf = b.asFun();
    if (af.params.size() != bf.params.size())
      throw UnifyError("arity mismatch: " + a.toString() + " vs " +
                       b.toString());
    Substitution s;
    for (size_t i = 0; i < af.params.size(); ++i) {
      auto si = unify(s.apply(af.params[i]), s.apply(bf.params[i]));
      s.compose(si);
    }
    auto sr = unify(s.apply(*af.ret), s.apply(*bf.ret));
    s.compose(sr);
    return s;
  }

  // Con vs Fun or any mismatch
  throw UnifyError("cannot unify " + a.toString() + " with " + b.toString());
}

} // namespace phi
