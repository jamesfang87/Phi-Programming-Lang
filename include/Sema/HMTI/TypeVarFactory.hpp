#pragma once

namespace phi {

// ---------------------------
// TypeVar generator
// ---------------------------
struct TypeVarFactory {
  int next = 0;
  int fresh() { return next++; }
};

} // namespace phi
