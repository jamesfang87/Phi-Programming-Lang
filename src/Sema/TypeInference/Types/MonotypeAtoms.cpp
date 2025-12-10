#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"

#include <expected>
#include <numeric>

#include "Diagnostics/Diagnostic.hpp"
#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"

namespace phi {

bool TypeVar::occursIn(const Monotype &M) const {
  if (M.isVar())
    return M.asVar() == *this;
  return M.freeTypeVars().contains(*this);
}

std::expected<Substitution, Diagnostic>
TypeVar::bindWith(const Monotype &M) const {
  // Trivial: identical variable
  if (M.isVar() && M.asVar() == *this)
    return Substitution{};

  // Occurs check — prevents recursive types like `T = T -> T`
  if (occursIn(M)) {
    auto D = error(std::format("cannot assign type variable `{}` to `{}` — "
                               "this would create an infinite type",
                               Id, M.toString()))
                 .with_primary_label(SrcSpan(M.getLocation()),
                                     "this type appears recursively here")
                 .with_note("add an explicit annotation to resolve the cycle")
                 .build();
    return std::unexpected(std::move(D));
  }

  // Constraint checking
  if (Constraints) {
    // Concrete type case
    if (M.isCon()) {
      const auto &Name = M.asCon().StringRep;
      if (!std::ranges::contains(*Constraints, Name)) {
        std::string Allowed =
            std::accumulate(std::next(Constraints->begin()), Constraints->end(),
                            Constraints->front(), [](auto A, auto &B) {
                              return std::move(A) + ", " + B;
                            });

        auto D =
            error(std::format("type `{}` does not satisfy required constraints",
                              Name))
                .with_primary_label(
                    SrcSpan(M.getLocation()),
                    std::format("`{}` is not permitted here", Name))
                .with_note(std::format("allowed types: {}", Allowed))
                .build();
        return std::unexpected(std::move(D));
      }
    }

    // Variable–Variable constraint compatibility
    else if (M.isVar() && M.asVar().Constraints) {
      const auto &A = *Constraints;
      const auto &B = *M.asVar().Constraints;

      std::vector<std::string> Shared;
      std::ranges::set_intersection(A, B, std::back_inserter(Shared));

      if (Shared.empty()) {
        auto Join = [](const std::vector<std::string> &V) {
          return std::accumulate(
              std::next(V.begin()), V.end(), V.front(),
              [](auto A, auto &B) { return std::move(A) + ", " + B; });
        };

        auto D = error("incompatible type constraints between type variables")
                     .with_note(std::format("left allows: {}", Join(A)))
                     .with_note(std::format("right allows: {}", Join(B)))
                     .with_note("consider adding an explicit type annotation "
                                "to disambiguate")
                     .build();
        return std::unexpected(std::move(D));
      }
    }
  }

  // Success
  Substitution S;
  S.Map.emplace(*this, M);
  return S;
}

bool TypeCon::operator==(const TypeCon &Rhs) const { return Data == Rhs.Data; }

bool TypeCon::operator!=(const TypeCon &Rhs) const { return !(*this == Rhs); }

bool TypeApp::operator==(const TypeApp &Rhs) const {
  return AppKind == Rhs.AppKind && Args == Rhs.Args;
}

bool TypeApp::operator!=(const TypeApp &Rhs) const { return !(*this == Rhs); }

bool TypeFun::operator==(const TypeFun &Rhs) const {
  // two TypeFun are "equal" (read compatible) if they have the same arity
  return Params.size() == Rhs.Params.size();
}

bool TypeFun::operator!=(const TypeFun &Rhs) const { return !(*this == Rhs); }

} // namespace phi
