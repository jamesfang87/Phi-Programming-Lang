#include "Sema/TypeInference/Substitution.hpp"

#include <ranges>

namespace phi {

// Apply to a type
[[nodiscard]] Monotype Substitution::apply(const Monotype &M) const {
  struct Visitor {
    const Substitution *Self;
    const Monotype &Orig;

    Monotype operator()(const TypeVar &Var) const {
      const auto It = Self->Map.find(Var);
      return (It != Self->Map.end()) ? Self->apply(It->second) : Orig;
    }

    Monotype operator()(const TypeCon &Con) const {
      (void)Con;
      return Orig;
    }

    Monotype operator()(const TypeApp &App) const {
      if (App.Args.empty())
        return Orig;
      std::vector<Monotype> Args;
      Args.reserve(App.Args.size());
      for (const auto &Arg : App.Args)
        Args.push_back(Self->apply(Arg));
      return Monotype::makeApp(App.AppKind, std::move(Args));
    }

    Monotype operator()(const TypeFun &Fun) const {
      std::vector<Monotype> Params;
      Params.reserve(Fun.Params.size());
      for (const auto &Param : Fun.Params)
        Params.push_back(Self->apply(Param));
      return Monotype::makeFun(std::move(Params), Self->apply(*Fun.Ret));
    }
  };

  return std::visit(Visitor{.Self = this, .Orig = M}, M.getPtr());
}

// Apply to a scheme (don’t touch quantified vars)
[[nodiscard]] Polytype Substitution::apply(const Polytype &P) const {
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
void Substitution::compose(const Substitution &Other) {
  if (Other.empty())
    return;

  // this := S2 ∘ this
  for (auto &T : Map | std::views::values)
    T = Other.apply(T);
  for (const auto &[TypeVar, Monotype] : Other.Map)
    Map.insert_or_assign(TypeVar, Monotype);
}

} // namespace phi
