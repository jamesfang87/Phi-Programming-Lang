#pragma once

namespace phi {

// ---------------------------
// TypeVar generator
// ---------------------------
struct TypeVarFactory {
  int Next = 0;
  int fresh() { return Next++; }
};

} // namespace phi
