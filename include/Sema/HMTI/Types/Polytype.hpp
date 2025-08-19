#pragma once

#include "Sema/HMTI/Types/Monotype.hpp"

namespace phi {

class Polytype {
public:
  Polytype(std::vector<TypeVar> quant, Monotype body)
      : quant(std::move(quant)), body(std::move(body)) {}

  [[nodiscard]] const std::vector<TypeVar> &getQuant() const { return quant; }
  [[nodiscard]] const Monotype &getBody() const { return body; }

  const std::unordered_set<TypeVar> freeTypeVars() const {
    auto sftv = body.freeTypeVars();
    for (auto &q : quant)
      sftv.erase(q);
    return sftv;
  }

private:
  std::vector<TypeVar> quant;
  Monotype body;
};

} // namespace phi
