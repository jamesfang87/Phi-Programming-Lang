#pragma once
#include <algorithm>
#include <format>
#include <memory>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <vector>

#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"

namespace phi {

// ---------------------------
// Unification
// ---------------------------
struct UnifyError : std::runtime_error {
  using std::runtime_error::runtime_error;
};

inline bool occurs(const TypeVar &X, const Monotype &M) {
  if (M.isVar())
    return M.asVar() == X;
  return M.freeTypeVars().contains(X);
}

inline Substitution bindVar(const TypeVar &X, const Monotype &M) {
  if (M.isVar() && M.asVar() == X)
    return {};
  if (occurs(X, M))
    throw UnifyError(
        std::format("occurs check failed: {} in {}", X.Id, M.toString()));

  // Check constraints
  if (X.Constraints) {
    if (M.isCon()) {
      const std::string &Name = M.asCon().Name;
      if (X.Constraints && !std::ranges::contains(*X.Constraints, Name)) {
        std::string Msg = std::format("type constraint violation: "
                                      "found type {} cannot be "
                                      "unified with expected types of: ",
                                      Name);
        for (const auto &Possible : *X.Constraints) {
          Msg += Possible + ", ";
        }
        throw UnifyError(Msg);
      }
    } else if (M.isVar() && M.asVar().Constraints) {
      const auto &[Id, Constraints] = M.asVar();
      // Check if constraints are compatible
      std::vector<std::string> Intersect;
      std::ranges::set_intersection(*X.Constraints, *Constraints,
                                    std::back_inserter(Intersect));
      if (Intersect.empty()) {
        throw UnifyError("incompatible type constraints");
      }
    }
  }

  Substitution Subst;
  Subst.Map.emplace(X, M);
  return Subst;
}

inline Substitution unify(const Monotype &T1, const Monotype &T2) {
  if (T1.isVar())
    return bindVar(T1.asVar(), T2);
  if (T2.isVar())
    return bindVar(T2.asVar(), T1);

  if (T1.isCon() && T2.isCon()) {
    const auto &Con1 = T1.asCon(), &Con2 = T2.asCon();
    if (Con1.Name != Con2.Name || Con1.Args.size() != Con2.Args.size())
      throw UnifyError("cannot unify " + T1.toString() + " with " +
                       T2.toString());
    Substitution Subst;
    for (size_t I = 0; I < Con1.Args.size(); ++I) {
      auto Arg1 = Con1.Args[I], Arg2 = Con2.Args[I];
      auto ArgTypeSubst = unify(Subst.apply(Arg1), Subst.apply(Arg2));
      Subst.compose(ArgTypeSubst);
    }
    return Subst;
  }

  if (T1.isFun() && T2.isFun()) {
    const auto &Fun1 = T1.asFun(), &Fun2 = T2.asFun();
    if (Fun1.Params.size() != Fun2.Params.size())
      throw UnifyError("param length mismatch: " + T1.toString() + " vs " +
                       T2.toString());
    Substitution Subst;
    for (size_t I = 0; I < Fun1.Params.size(); ++I) {
      auto Param1 = Fun1.Params[I], Param2 = Fun2.Params[I];
      auto ParamTypeSubst = unify(Subst.apply(Param1), Subst.apply(Param2));
      Subst.compose(ParamTypeSubst);
    }
    auto RetTypeSubst = unify(Subst.apply(*Fun1.Ret), Subst.apply(*Fun2.Ret));
    Subst.compose(RetTypeSubst);
    return Subst;
  }

  // Con vs Fun or any mismatch
  throw UnifyError("Cannot unify " + T1.toString() + " with " + T2.toString());
}

} // namespace phi
