#include "Sema/NameResolution/SymbolTable.hpp"

#include <algorithm>
#include <limits>
#include <optional>
#include <ranges>
#include <string>
#include <unordered_map>
#include <vector>

namespace phi {

// ---------- Damerau–Levenshtein (anonymous namespace) ----------
namespace {

using size_t_t = std::size_t;

size_t_t damerauLevenshtein(const std::string &A, const std::string &B) {
  const size_t_t LenA = A.size();
  const size_t_t LenB = B.size();

  if (LenA == 0)
    return LenB;
  if (LenB == 0)
    return LenA;

  const size_t_t INF = LenA + LenB;
  std::vector<std::vector<size_t_t>> DP(LenA + 2,
                                        std::vector<size_t_t>(LenB + 2, 0));
  DP[0][0] = INF;
  for (size_t_t i = 0; i <= LenA; ++i) {
    DP[i + 1][1] = i;
    DP[i + 1][0] = INF;
  }
  for (size_t_t j = 0; j <= LenB; ++j) {
    DP[1][j + 1] = j;
    DP[0][j + 1] = INF;
  }

  std::unordered_map<char, size_t_t> Last;
  for (size_t_t i = 1; i <= LenA; ++i) {
    size_t_t LastMatchCol = 0;
    for (size_t_t j = 1; j <= LenB; ++j) {
      const size_t_t i1 = Last.count(B[j - 1]) ? Last[B[j - 1]] : 0;
      const size_t_t j1 = LastMatchCol;
      const size_t_t Cost = (A[i - 1] == B[j - 1]) ? 0 : 1;
      if (Cost == 0)
        LastMatchCol = j;

      DP[i + 1][j + 1] = std::min({
          DP[i][j] + Cost,                             // substitution
          DP[i + 1][j] + 1,                            // insertion
          DP[i][j + 1] + 1,                            // deletion
          DP[i1][j1] + (i - i1 - 1) + 1 + (j - j1 - 1) // transposition
      });
    }
    Last[A[i - 1]] = i;
  }

  return DP[LenA + 1][LenB + 1];
}

// Decide whether a found distance is "good enough" as a suggestion.
// We make threshold dynamic: short identifiers get small thresholds,
// longer identifiers allow slightly larger distances (but capped).
bool IsDistanceGoodEnough(std::size_t Distance, const std::string &Query) {
  std::size_t Thr = std::max<std::size_t>(1, Query.size() / 3);
  Thr = std::min<std::size_t>(Thr, 4); // cap so suggestions don't become noisy
  return Distance <= Thr;
}

} // namespace

// ---------- Primitive names ----------
static const std::vector<std::string> PrimitiveNames = {
    "i8",  "i16", "i32", "i64",    "u8",   "u16",  "u32",
    "u64", "f32", "f64", "string", "char", "bool", "range"};

// ---------- Closest helpers ----------
FunDecl *SymbolTable::getClosestFun(const std::string &Undeclared) const {
  FunDecl *Best = nullptr;
  std::size_t BestDist = std::numeric_limits<std::size_t>::max();

  // Prefer innermost scopes first (iterate from back to front)
  for (const auto &ScopeRef : std::ranges::reverse_view(Scopes)) {
    for (const auto &Pair : ScopeRef.Funs) {
      const std::string &CandName = Pair.first;
      std::size_t Dist = damerauLevenshtein(Undeclared, CandName);
      if (Dist < BestDist) {
        BestDist = Dist;
        Best = Pair.second;
      }
    }
  }

  if (Best && IsDistanceGoodEnough(BestDist, Undeclared))
    return Best;
  return nullptr;
}

AdtDecl *SymbolTable::getClosestAdt(const std::string &Undeclared) const {
  AdtDecl *Best = nullptr;
  std::size_t BestDist = std::numeric_limits<std::size_t>::max();

  // Prefer innermost scopes first (iterate from back to front)
  for (const auto &ScopeRef : std::ranges::reverse_view(Scopes)) {
    for (const auto &Pair : ScopeRef.Adts) {
      const std::string &CandName = Pair.first;
      std::size_t Dist = damerauLevenshtein(Undeclared, CandName);
      if (Dist < BestDist) {
        BestDist = Dist;
        Best = Pair.second;
      }
    }
  }

  if (Best && IsDistanceGoodEnough(BestDist, Undeclared))
    return Best;
  return nullptr;
}

LocalDecl *SymbolTable::getClosestLocal(const std::string &Undeclared) const {
  LocalDecl *Best = nullptr;
  std::size_t BestDist = std::numeric_limits<std::size_t>::max();

  // Prefer innermost scopes first (iterate from back to front)
  for (const auto &ScopeRef : std::ranges::reverse_view(Scopes)) {
    for (const auto &Pair : ScopeRef.Vars) {
      const std::string &CandName = Pair.first;
      std::size_t Dist = damerauLevenshtein(Undeclared, CandName);
      if (Dist < BestDist) {
        BestDist = Dist;
        Best = Pair.second;
      }
    }
  }

  if (Best && IsDistanceGoodEnough(BestDist, Undeclared))
    return Best;
  return nullptr;
}

std::optional<std::string>
SymbolTable::getClosestType(const std::string &Undeclared) const {
  std::string BestName;
  std::size_t BestDist = std::numeric_limits<std::size_t>::max();

  // 1) primitives
  for (const auto &Prim : PrimitiveNames) {
    std::size_t Dist = damerauLevenshtein(Undeclared, Prim);
    if (Dist < BestDist) {
      BestDist = Dist;
      BestName = Prim;
    }
  }

  for (const auto &ScopeRef : std::ranges::reverse_view(Scopes)) {
    // Check structs
    for (const auto &Pair : ScopeRef.Adts) {
      const std::string &CandName = Pair.first;
      std::size_t Dist = damerauLevenshtein(Undeclared, CandName);
      if (Dist < BestDist) {
        BestDist = Dist;
        BestName = CandName;
      }
    }
  }

  if (!BestName.empty() && IsDistanceGoodEnough(BestDist, Undeclared))
    return BestName;

  return std::nullopt;
}

} // namespace phi
