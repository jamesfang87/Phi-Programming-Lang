#include "Diagnostics/DiagnosticBuilder.hpp"
#include "Sema/TypeInference/Infer.hpp"
#include "Sema/TypeInference/Substitution.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"

namespace phi {

void TypeInferencer::unifyInto(Substitution &S, const Monotype &A,
                               const Monotype &B) {
  Substitution U = unify(S.apply(A), S.apply(B));
  S.compose(U);
}

Substitution TypeInferencer::unify(const Monotype &A, const Monotype &B) {
  // When both are variables, prefer to bind unconstrained to constrained
  if (A.isVar() && B.isVar()) {
    const auto &VarA = A.asVar();
    const auto &VarB = B.asVar();

    // If A is constrained and B is not, bind B to A
    if (VarA.Constraints && !VarB.Constraints) {
      return unifyVar(B, A);
    }
    // If B is constrained and A is not, bind A to B
    if (VarB.Constraints && !VarA.Constraints) {
      return unifyVar(A, B);
    }
    // If both have constraints, check compatibility in bindWith
    // Default to binding A to B
    return unifyVar(A, B);
  }

  if (B.isVar()) {
    return unifyVar(B, A);
  }

  struct UnifyVisitor {
    TypeInferencer &TI;
    const Monotype &A;
    const Monotype &B;

    // TypeVar case
    Substitution operator()(const TypeVar &_) const {
      return TI.unifyVar(A, B);
    }

    // TypeCon case
    Substitution operator()(const TypeCon &_) const {
      return TI.unifyCon(A, B);
    }

    // TypeApp case
    Substitution operator()(const TypeApp &_) const {
      return TI.unifyApp(A, B);
    }

    // TypeFun case
    Substitution operator()(const TypeFun &_) const {
      return TI.unifyFun(A, B);
    }
  };
  return std::visit(UnifyVisitor{.TI = *this, .A = A, .B = B}, A.getPtr());
}

Substitution TypeInferencer::unifyVar(const Monotype &Var, const Monotype &B) {
  if (Var.isVar()) {
    auto varConstraints = Var.asVar().Constraints;
  }
  if (B.isVar()) {
    auto bConstraints = B.asVar().Constraints;
  }

  auto Res = Var.asVar().bindWith(B);
  if (Res) {
    return *Res;
  }

  DiagMan->emit(Res.error());
  return Substitution{};
}

Substitution TypeInferencer::unifyCon(const Monotype &A, const Monotype &B) {
  if (!B.isCon()) {
    emitUnifyError(
        A, B,
        std::format("cannot unify `{}` with `{}`", A.toString(), B.toString()));
    return Substitution{};
  }

  if (A.asCon() != B.asCon()) {
    emitUnifyError(
        A, B,
        std::format("cannot unify `{}` with `{}`", A.toString(), B.toString()));
    return Substitution{};
  }

  Substitution S;
  for (size_t i = 0; i < B.asCon().Args.size(); ++i) {
    auto ArgA = A.asCon().Args[i];
    auto ArgB = B.asCon().Args[i];
    auto Sub = unify(S.apply(ArgA), S.apply(ArgB));
    S.compose(Sub);
  }
  return S;
}

Substitution TypeInferencer::unifyApp(const Monotype &A, const Monotype &B) {
  if (!B.isApp()) {
    emitUnifyError(
        A, B,
        std::format("cannot unify `{}` with `{}`", A.toString(), B.toString()));
    return Substitution{};
  }

  if (A.asApp() != B.asApp()) {
    emitUnifyError(
        A, B,
        std::format("cannot unify `{}` with `{}`", A.toString(), B.toString()));
    return Substitution{};
  }

  Substitution S;
  for (size_t i = 0; i < B.asApp().Args.size(); ++i) {
    auto ArgA = A.asApp().Args[i];
    auto ArgB = B.asApp().Args[i];
    auto Sub = unify(S.apply(ArgA), S.apply(ArgB));
    S.compose(Sub);
  }
  return S;
}

Substitution TypeInferencer::unifyFun(const Monotype &A, const Monotype &B) {
  if (!B.isFun()) {
    emitUnifyError(
        A, B,
        std::format("cannot unify `{}` with `{}`", A.toString(), B.toString()));
    return Substitution{};
  }

  if (A.asFun() != B.asFun()) {
    emitUnifyError(
        A, B,
        std::format("function arity mismatch: {} vs {}",
                    A.asFun().Params.size(), B.asFun().Params.size()),
        std::optional<std::string>("check the number of parameters"));
    return Substitution{};
  }

  Substitution S;
  for (size_t i = 0; i < A.asFun().Params.size(); ++i) {
    auto ParamA = A.asFun().Params[i];
    auto ParamB = B.asFun().Params[i];
    auto ParamSubst = unify(S.apply(ParamA), S.apply(ParamB));
    S.compose(ParamSubst);
  }

  auto RetSubst = unify(S.apply(*A.asFun().Ret), S.apply(*B.asFun().Ret));
  S.compose(RetSubst);
  return S;
}

void TypeInferencer::emitUnifyError(const Monotype &A, const Monotype &B,
                                    const std::string &TopMsg,
                                    const std::optional<std::string> &Note) {
  auto Builder = error(TopMsg);

  if (A.getLocation().Line >= 0) {
    Builder = Builder.with_primary_label(
        SrcSpan(A.getLocation()),
        std::format("`{}` inferred/used here", A.toString()));
    if (B.getLocation().Line >= 0) {
      Builder = Builder.with_secondary_label(
          SrcSpan(B.getLocation()),
          std::format("conflicts with `{}` here", B.toString()));
    }
  } else if (B.getLocation().Line >= 0) {
    Builder = Builder.with_primary_label(
        SrcSpan(B.getLocation()),
        std::format("`{}` inferred/used here", B.toString()));
  }

  if (Note)
    Builder = Builder.with_note(*Note);

  Builder.emit(*DiagMan);
}

} // namespace phi
