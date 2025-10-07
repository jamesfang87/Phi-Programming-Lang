#pragma once

#include "Sema/TypeInference/TypeVarFactory.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"

namespace phi {

class Polytype {
public:
  Polytype(std::vector<TypeVar> Quant, Monotype Body)
      : Quant(std::move(Quant)), Body(std::move(Body)) {}

  [[nodiscard]] const std::vector<TypeVar> &getQuant() const { return Quant; }
  [[nodiscard]] const Monotype &getBody() const { return Body; }
  [[nodiscard]] std::unordered_set<TypeVar> freeTypeVars() const;
  Monotype instantiate(TypeVarFactory &Factory);

private:
  std::vector<TypeVar> Quant;
  Monotype Body;
};

} // namespace phi
