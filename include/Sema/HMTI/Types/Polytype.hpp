#pragma once

#include "Sema/HMTI/Types/Monotype.hpp"

namespace phi {

class Polytype {
public:
  Polytype(std::vector<TypeVar> Quant, Monotype Body)
      : Quant(std::move(Quant)), Body(std::move(Body)) {}

  [[nodiscard]] const std::vector<TypeVar> &getQuant() const { return Quant; }
  [[nodiscard]] const Monotype &getBody() const { return Body; }

  const std::unordered_set<TypeVar> freeTypeVars() const {
    auto FreeTypeVars = Body.freeTypeVars();
    for (auto &Q : Quant)
      FreeTypeVars.erase(Q);
    return FreeTypeVars;
  }

private:
  std::vector<TypeVar> Quant;
  Monotype Body;
};

} // namespace phi
