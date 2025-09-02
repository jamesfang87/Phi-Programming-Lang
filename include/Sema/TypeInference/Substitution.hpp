#pragma once

#include <ranges>
#include <unordered_map>

#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

// ---------------------------
// Substitution
// ---------------------------
struct Substitution {
  // v ↦ type
  std::unordered_map<TypeVar, Monotype> Map;

  [[nodiscard]] bool empty() const { return Map.empty(); }

  // Apply to a type
  [[nodiscard]] Monotype apply(const Monotype &M) const {
    return M.visit(
        [&](const TypeVar &Var) -> Monotype {
          const auto It = Map.find(Var);
          return (It != Map.end()) ? apply(It->second) : M;
        },
        [&](const TypeCon &Con) -> Monotype {
          if (Con.Args.empty())
            return M;
          std::vector<Monotype> Args;
          Args.reserve(Con.Args.size());
          for (auto &Arg : Con.Args)
            Args.push_back(apply(Arg));
          return Monotype::makeCon(Con.Name, std::move(Args));
        },
        [&](const TypeApp &App) -> Monotype {
          if (App.Args.empty())
            return M;
          std::vector<Monotype> Args;
          Args.reserve(App.Args.size());
          for (auto &Arg : App.Args)
            Args.push_back(apply(Arg));
          return Monotype::makeApp(App.Name, std::move(Args));
        },
        [&](const TypeFun &Fun) -> Monotype {
          std::vector<Monotype> Params;
          Params.reserve(Fun.Params.size());
          for (auto &Param : Fun.Params)
            Params.push_back(apply(Param));
          return Monotype::makeFun(std::move(Params), apply(*Fun.Ret));
        });
  }

  // Apply to a scheme (don’t touch quantified vars)
  [[nodiscard]] Polytype apply(const Polytype &P) const {
    if (Map.empty() || P.getQuant().empty())
      return {P.getQuant(), apply(P.getBody())};

    Substitution Filtered;
    for (const auto &[TypeVar, Monotype] : Map) {
      bool Quantified = false;
      for (auto &Q : P.getQuant())
        if (Q == TypeVar) {
          Quantified = true;
          break;
        }
      if (!Quantified)
        Filtered.Map.emplace(TypeVar, Monotype);
    }
    return Polytype{P.getQuant(), Filtered.apply(P.getBody())};
  }

  // Compose: this := s2 ∘ this   (apply s2 after this)
  void compose(const Substitution &Other) {
    if (Other.empty())
      return;

    // this := S2 ∘ this
    for (auto &T : Map | std::views::values)
      T = Other.apply(T);
    for (const auto &[TypeVar, Monotype] : Other.Map)
      Map.insert_or_assign(TypeVar, Monotype);
  }
};

} // namespace phi
