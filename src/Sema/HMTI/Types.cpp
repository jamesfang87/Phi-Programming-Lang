// src/Sema/HMTI/Types.cpp
#include "Sema/HMTI/HMType.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <algorithm>
#include <format>
#include <functional>
#include <string>

namespace phi {

// Monotype factories
std::shared_ptr<Monotype> Monotype::var(TypeVar V) {
  // Use direct new here because std::make_shared requires accessible
  // constructor
  std::shared_ptr<Monotype> P(new Monotype());
  P->K = Kind::Var;
  P->V = std::move(V);
  return P;
}

std::shared_ptr<Monotype>
Monotype::con(std::string Name, std::vector<std::shared_ptr<Monotype>> Args) {
  std::shared_ptr<Monotype> P(new Monotype());
  P->K = Kind::Con;
  P->ConName = std::move(Name);
  P->ConArgs = std::move(Args);
  return P;
}

std::shared_ptr<Monotype>
Monotype::fun(std::vector<std::shared_ptr<Monotype>> Params,
              std::shared_ptr<Monotype> Ret) {
  std::shared_ptr<Monotype> P(new Monotype());
  P->K = Kind::Fun;
  P->FunArgs = std::move(Params);
  P->FunRet = std::move(Ret);
  return P;
}

void Monotype::setFun(std::vector<std::shared_ptr<Monotype>> Params,
                      std::shared_ptr<Monotype> Ret) {
  K = Kind::Fun;
  FunArgs = std::move(Params);
  FunRet = std::move(Ret);
}

// free type vars
static void collectFv(const std::shared_ptr<Monotype> &T,
                      std::unordered_set<TypeVar, TypeVarHash> &Out) {
  switch (T->tag()) {
  case Monotype::Kind::Var:
    Out.insert(T->asVar());
    break;
  case Monotype::Kind::Con:
    for (auto &A : T->getConArgs())
      collectFv(A, Out);
    break;
  case Monotype::Kind::Fun:
    for (auto &A : T->getFunArgs())
      collectFv(A, Out);
    collectFv(T->getFunReturn(), Out);
    break;
  }
}

std::unordered_set<TypeVar, TypeVarHash>
freeTypeVars(const std::shared_ptr<Monotype> &T) {
  std::unordered_set<TypeVar, TypeVarHash> S;
  collectFv(T, S);
  return S;
}

std::unordered_set<TypeVar, TypeVarHash> freeTypeVars(const Polytype &S) {
  auto F = freeTypeVars(S.getBody());
  for (auto &q : S.getQuant())
    F.erase(q);
  return F;
}

static std::shared_ptr<Monotype> applyOne(
    const std::unordered_map<TypeVar, std::shared_ptr<Monotype>, TypeVarHash>
        &M,
    const std::shared_ptr<Monotype> &T) {
  if (!T) {
    throw std::runtime_error(
        "internal error: applying substitution to null Monotype");
  }

  switch (T->tag()) {
  case Monotype::Kind::Var: {
    auto It = M.find(T->asVar());
    return It == M.end() ? T : It->second;
  }
  case Monotype::Kind::Con: {
    std::vector<std::shared_ptr<Monotype>> Args;
    Args.reserve(T->getConArgs().size());
    for (auto &A : T->getConArgs())
      Args.push_back(applyOne(M, A));
    return Monotype::con(T->getConName(), std::move(Args));
  }
  case Monotype::Kind::Fun: {
    std::vector<std::shared_ptr<Monotype>> Args;
    Args.reserve(T->getFunArgs().size());
    for (auto &A : T->getFunArgs())
      Args.push_back(applyOne(M, A));
    return Monotype::fun(std::move(Args), applyOne(M, T->getFunReturn()));
  }
  }
  return T;
}

std::shared_ptr<Monotype>
Substitution::apply(const std::shared_ptr<Monotype> &T) const {
  return applyOne(Map, T);
}

Polytype Substitution::apply(const Polytype &S) const {
  Substitution Filtered;
  for (auto &KV : Map) {
    bool Quantified = false;
    for (auto &Q : S.getQuant())
      if (Q == KV.first) {
        Quantified = true;
        break;
      }
    if (!Quantified)
      Filtered.Map.emplace(KV.first, KV.second);
  }
  return Polytype{S.getQuant(), Filtered.apply(S.getBody())};
}

void Substitution::compose(const Substitution &S2) {
  // this := S2 ∘ this
  for (auto &KV : Map)
    KV.second = S2.apply(KV.second);
  for (auto &KV : S2.Map)
    Map.insert_or_assign(KV.first, KV.second);
}

// instantiate
std::shared_ptr<Monotype> instantiate(const Polytype &S, TypeVarFactory &F) {
  std::unordered_map<int, std::shared_ptr<Monotype>> VMap;
  for (auto &q : S.getQuant())
    VMap.emplace(q.Id, Monotype::var(F.fresh()));
  std::function<std::shared_ptr<Monotype>(const std::shared_ptr<Monotype> &)>
      go =
          [&](const std::shared_ptr<Monotype> &T) -> std::shared_ptr<Monotype> {
    switch (T->tag()) {
    case Monotype::Kind::Var: {
      auto it = VMap.find(T->asVar().Id);
      return it == VMap.end() ? T : it->second;
    }
    case Monotype::Kind::Con: {
      std::vector<std::shared_ptr<Monotype>> Args;
      Args.reserve(T->getConArgs().size());
      for (auto &A : T->getConArgs())
        Args.push_back(go(A));
      return Monotype::con(T->getConName(), std::move(Args));
    }
    case Monotype::Kind::Fun: {
      std::vector<std::shared_ptr<Monotype>> Args;
      Args.reserve(T->getFunArgs().size());
      for (auto &A : T->getFunArgs())
        Args.push_back(go(A));
      return Monotype::fun(std::move(Args), go(T->getFunReturn()));
    }
    }
    return T;
  };
  return go(S.getBody());
}

// generalize
Polytype generalize(const TypeEnv &Env, const std::shared_ptr<Monotype> &T) {
  auto ft = freeTypeVars(T);
  auto fe = freeTypeVars(Env);
  std::vector<TypeVar> q;
  for (auto &v : ft)
    if (!fe.count(v))
      q.push_back(v);
  return Polytype{std::move(q), T};
}

// unify
static bool occurs(const TypeVar &V, const std::shared_ptr<Monotype> &T) {
  return freeTypeVars(T).find(V) != freeTypeVars(T).end();
}

static Substitution bindVar(const TypeVar &V,
                            const std::shared_ptr<Monotype> &T) {
  if (T->tag() == Monotype::Kind::Var && T->asVar().Id == V.Id)
    return {};
  if (occurs(V, T))
    throw UnifyError("occurs check failed");

  // Check constraints
  if (V.Constraints) {
    if (T->tag() == Monotype::Kind::Con) {
      const std::string &name = T->getConName();
      if (V.Constraints && !std::ranges::contains(*V.Constraints, name)) {
        std::string Msg =
            std::format("type constraint violation: found type {} cannot be "
                        "unified with expected types of:",
                        name);
        for (const auto &Possible : *V.Constraints) {
          Msg += Possible + ", ";
        }
        throw UnifyError(Msg);
      }
    } else if (T->tag() == Monotype::Kind::Var && T->asVar().Constraints) {
      // Check if constraints are compatible
      std::vector<std::string> common;
      std::set_intersection(V.Constraints->begin(), V.Constraints->end(),
                            T->asVar().Constraints->begin(),
                            T->asVar().Constraints->end(),
                            std::back_inserter(common));
      if (common.empty()) {
        throw UnifyError("incompatible type constraints");
      }
    }
  }

  Substitution S;
  S.Map.emplace(V, T);
  return S;
}

Substitution unify(const std::shared_ptr<Monotype> &A,
                   const std::shared_ptr<Monotype> &B) {
  if (A->tag() == Monotype::Kind::Var)
    return bindVar(A->asVar(), B);
  if (B->tag() == Monotype::Kind::Var)
    return bindVar(B->asVar(), A);

  if (A->tag() == Monotype::Kind::Con && B->tag() == Monotype::Kind::Con) {
    if (A->getConName() != B->getConName() ||
        A->getConArgs().size() != B->getConArgs().size()) {
      throw UnifyError("constructor mismatch: " + A->getConName() + " vs " +
                       B->getConName());
    }
    Substitution S;
    for (size_t i = 0; i < A->getConArgs().size(); ++i) {
      Substitution Si =
          unify(S.apply(A->getConArgs()[i]), S.apply(B->getConArgs()[i]));
      S.compose(Si);
    }
    return S;
  }

  if (A->tag() == Monotype::Kind::Fun && B->tag() == Monotype::Kind::Fun) {
    if (A->getFunArgs().size() != B->getFunArgs().size())
      throw UnifyError("arity mismatch");
    Substitution S;
    for (size_t i = 0; i < A->getFunArgs().size(); ++i) {
      Substitution Si =
          unify(S.apply(A->getFunArgs()[i]), S.apply(B->getFunArgs()[i]));
      S.compose(Si);
    }
    Substitution Sr =
        unify(S.apply(A->getFunReturn()), S.apply(B->getFunReturn()));
    S.compose(Sr);
    return S;
  }

  throw UnifyError("cannot unify given types");
}

Type Monotype::toAstType() const {
  // AST.Type doesn't have function type; it's only primitive/custom.
  if (K == Monotype::Kind::Var) {
    return Type("");
  }

  if (K == Monotype::Kind::Con) {
    const auto &Name = ConName;
    if (Name == "i8")
      return Type(Type::PrimitiveKind::I8Kind);
    if (Name == "i16")
      return Type(Type::PrimitiveKind::I16Kind);
    if (Name == "i32")
      return Type(Type::PrimitiveKind::I32Kind);
    if (Name == "i64")
      return Type(Type::PrimitiveKind::I64Kind);
    if (Name == "u8")
      return Type(Type::PrimitiveKind::U8Kind);
    if (Name == "u16")
      return Type(Type::PrimitiveKind::U16Kind);
    if (Name == "u32")
      return Type(Type::PrimitiveKind::U32Kind);
    if (Name == "u64")
      return Type(Type::PrimitiveKind::U64Kind);
    if (Name == "f32")
      return Type(Type::PrimitiveKind::F32Kind);
    if (Name == "f64")
      return Type(Type::PrimitiveKind::F64Kind);
    if (Name == "string")
      return Type(Type::PrimitiveKind::StringKind);
    if (Name == "char")
      return Type(Type::PrimitiveKind::CharKind);
    if (Name == "bool")
      return Type(Type::PrimitiveKind::BoolKind);
    if (Name == "range")
      return Type(Type::PrimitiveKind::RangeKind);
    if (Name == "null")
      return Type(Type::PrimitiveKind::NullKind);
    // otherwise treat as custom/struct name
    return Type(Name);
  }
  // Fun: map to return type only — callers (Infer::annotate) handle
  // params+return
  if (K == Monotype::Kind::Fun) {
    return FunRet->toAstType();
  }
  return Type(Type::PrimitiveKind::I32Kind);
}

} // namespace phi
