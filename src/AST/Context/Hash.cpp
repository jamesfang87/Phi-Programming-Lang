#include "AST/TypeSystem/Type.hpp"

namespace phi {

static inline uint64_t splitmix64(uint64_t x) noexcept {
  x += 0x9e3779b97f4a7c15ULL;
  x = (x ^ (x >> 30)) * 0xBF58476D1CE4E5B9ULL;
  x = (x ^ (x >> 27)) * 0x94D049BB133111EBULL;
  return x ^ (x >> 31);
}

std::size_t TupleKeyHash::operator()(TupleKey const &k) const noexcept {
  uint64_t h = 14695981039346656037ULL; // FNV offset (seed)
  // mix in size early to distinguish different-length tuples
  h = splitmix64(h ^ static_cast<uint64_t>(k.Elements.size()));

  // mix each pointer value
  uint64_t i = 0;
  for (const TypeRef &t : k.Elements) {
    // cast pointer to integer (portable)
    uint64_t v = reinterpret_cast<uintptr_t>(t.getPtr());
    // incorporate index into mix to avoid commutative collisions
    uint64_t mixed =
        splitmix64(v + 0x9e3779b97f4a7c15ULL + (i << 6) + (i >> 2));
    h ^= mixed;
    h = splitmix64(h);
    ++i;
  }

  // final avalanche
  h = splitmix64(h + 0x9e3779b97f4a7c15ULL);
  return static_cast<std::size_t>(h ^ (h >> 32));
}

std::size_t FunKeyHash::operator()(FunKey const &k) const noexcept {
  uint64_t h = 14695981039346656037ULL;
  // mix return type pointer first
  uint64_t rv = reinterpret_cast<uintptr_t>(k.Ret.getPtr());
  h = splitmix64(h ^ rv);
  // mix params count
  h = splitmix64(h ^ static_cast<uint64_t>(k.Params.size()));

  uint64_t i = 0;
  for (const TypeRef &t : k.Params) {
    uint64_t v = reinterpret_cast<uintptr_t>(t.getPtr());
    uint64_t mixed =
        splitmix64(v + 0x9e3779b97f4a7c15ULL + (i << 6) + (i >> 2));
    h ^= mixed;
    h = splitmix64(h);
    ++i;
  }

  h = splitmix64(h + 0x9e3779b97f4a7c15ULL);
  return static_cast<std::size_t>(h ^ (h >> 32));
}

} // namespace phi
