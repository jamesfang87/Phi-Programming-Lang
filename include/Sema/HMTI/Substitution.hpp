#pragma once

#include "Sema/HMTI/Types/Monotype.hpp"
#include "Sema/HMTI/Types/Polytype.hpp"

#include <unordered_map>

namespace phi {

// ---------------------------
// Substitution
// ---------------------------
struct Substitution {
  // v ↦ type
  std::unordered_map<TypeVar, Monotype> Map;

  bool empty() const { return Map.empty(); }

  // Apply to a type
  Monotype apply(const Monotype &t) const {
    return t.visit(
        [&](const TypeVar &v) -> Monotype {
          auto it = Map.find(v);
          return (it != Map.end()) ? apply(it->second) : t;
        },
        [&](const TypeCon &c) -> Monotype {
          if (c.Args.empty())
            return t;
          std::vector<Monotype> args;
          args.reserve(c.Args.size());
          for (auto &a : c.Args)
            args.push_back(apply(a));
          return Monotype::makeCon(c.Name, std::move(args));
        },
        [&](const TypeFun &f) -> Monotype {
          std::vector<Monotype> ps;
          ps.reserve(f.Params.size());
          for (auto &p : f.Params)
            ps.push_back(apply(p));
          return Monotype::makeFun(std::move(ps), apply(*f.Ret));
        });
  }

  // Apply to a scheme (don’t touch quantified vars)
  Polytype apply(const Polytype &s) const {
    if (Map.empty() || s.getQuant().empty())
      return {s.getQuant(), apply(s.getBody())};

    Substitution Filtered;
    for (auto &KV : Map) {
      bool Quantified = false;
      for (auto &Q : s.getQuant())
        if (Q == KV.first) {
          Quantified = true;
          break;
        }
      if (!Quantified)
        Filtered.Map.emplace(KV.first, KV.second);
    }
    return Polytype{s.getQuant(), Filtered.apply(s.getBody())};
  }

  // Compose: this := s2 ∘ this   (apply s2 after this)
  void compose(const Substitution &s2) {
    if (s2.Map.empty())
      return;

    // this := S2 ∘ this
    for (auto &KV : Map)
      KV.second = s2.apply(KV.second);
    for (auto &KV : s2.Map)
      Map.insert_or_assign(KV.first, KV.second);
  }
};

} // namespace phi
