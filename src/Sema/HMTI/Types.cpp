// src/Sema/HMTI/Types.cpp
#include "Sema/HMTI/HMType.hpp"
#include "Sema/HMTI/TypeEnv.hpp"

#include <functional>

namespace phi {

// Monotype factories
std::shared_ptr<Monotype> Monotype::var(TypeVar V) {
  // Use direct new here because std::make_shared requires accessible
  // constructor
  std::shared_ptr<Monotype> P(new Monotype());
  P->Tag_ = Tag::Var;
  P->V_ = V;
  return P;
}

std::shared_ptr<Monotype>
Monotype::con(std::string Name, std::vector<std::shared_ptr<Monotype>> Args) {
  std::shared_ptr<Monotype> P(new Monotype());
  P->Tag_ = Tag::Con;
  P->ConName_ = std::move(Name);
  P->ConArgs_ = std::move(Args);
  return P;
}

std::shared_ptr<Monotype>
Monotype::fun(std::vector<std::shared_ptr<Monotype>> Params,
              std::shared_ptr<Monotype> Ret) {
  std::shared_ptr<Monotype> P(new Monotype());
  P->Tag_ = Tag::Fun;
  P->FunArgs_ = std::move(Params);
  P->FunRet_ = std::move(Ret);
  return P;
}

void Monotype::setFun(std::vector<std::shared_ptr<Monotype>> Params,
                      std::shared_ptr<Monotype> Ret) {
  Tag_ = Tag::Fun;
  FunArgs_ = std::move(Params);
  FunRet_ = std::move(Ret);
}

// free type vars
static void collectFv(const std::shared_ptr<Monotype> &T,
                      std::unordered_set<TypeVar, TypeVarHash> &Out) {
  switch (T->tag()) {
  case Monotype::Tag::Var:
    Out.insert(T->asVar());
    break;
  case Monotype::Tag::Con:
    for (auto &A : T->conArgs())
      collectFv(A, Out);
    break;
  case Monotype::Tag::Fun:
    for (auto &A : T->funArgs())
      collectFv(A, Out);
    collectFv(T->funRet(), Out);
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
  auto F = freeTypeVars(S.body());
  for (auto &q : S.quant())
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
  case Monotype::Tag::Var: {
    auto It = M.find(T->asVar());
    return It == M.end() ? T : It->second;
  }
  case Monotype::Tag::Con: {
    std::vector<std::shared_ptr<Monotype>> Args;
    Args.reserve(T->conArgs().size());
    for (auto &A : T->conArgs())
      Args.push_back(applyOne(M, A));
    return Monotype::con(T->conName(), std::move(Args));
  }
  case Monotype::Tag::Fun: {
    std::vector<std::shared_ptr<Monotype>> Args;
    Args.reserve(T->funArgs().size());
    for (auto &A : T->funArgs())
      Args.push_back(applyOne(M, A));
    return Monotype::fun(std::move(Args), applyOne(M, T->funRet()));
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
    for (auto &Q : S.quant())
      if (Q == KV.first) {
        Quantified = true;
        break;
      }
    if (!Quantified)
      Filtered.Map.emplace(KV.first, KV.second);
  }
  return Polytype{S.quant(), Filtered.apply(S.body())};
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
  for (auto &q : S.quant())
    VMap.emplace(q.Id, Monotype::var(F.fresh()));
  std::function<std::shared_ptr<Monotype>(const std::shared_ptr<Monotype> &)>
      go =
          [&](const std::shared_ptr<Monotype> &T) -> std::shared_ptr<Monotype> {
    switch (T->tag()) {
    case Monotype::Tag::Var: {
      auto it = VMap.find(T->asVar().Id);
      return it == VMap.end() ? T : it->second;
    }
    case Monotype::Tag::Con: {
      std::vector<std::shared_ptr<Monotype>> Args;
      Args.reserve(T->conArgs().size());
      for (auto &A : T->conArgs())
        Args.push_back(go(A));
      return Monotype::con(T->conName(), std::move(Args));
    }
    case Monotype::Tag::Fun: {
      std::vector<std::shared_ptr<Monotype>> Args;
      Args.reserve(T->funArgs().size());
      for (auto &A : T->funArgs())
        Args.push_back(go(A));
      return Monotype::fun(std::move(Args), go(T->funRet()));
    }
    }
    return T;
  };
  return go(S.body());
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
  if (T->tag() == Monotype::Tag::Var && T->asVar().Id == V.Id)
    return {};
  if (occurs(V, T))
    throw UnifyError("occurs check failed");
  Substitution S;
  S.Map.emplace(V, T);
  return S;
}

Substitution unify(const std::shared_ptr<Monotype> &A,
                   const std::shared_ptr<Monotype> &B) {
  if (A->tag() == Monotype::Tag::Var)
    return bindVar(A->asVar(), B);
  if (B->tag() == Monotype::Tag::Var)
    return bindVar(B->asVar(), A);

  if (A->tag() == Monotype::Tag::Con && B->tag() == Monotype::Tag::Con) {
    if (A->conName() != B->conName() ||
        A->conArgs().size() != B->conArgs().size()) {
      throw UnifyError("constructor mismatch: " + A->conName() + " vs " +
                       B->conName());
    }
    Substitution S;
    for (size_t i = 0; i < A->conArgs().size(); ++i) {
      Substitution Si =
          unify(S.apply(A->conArgs()[i]), S.apply(B->conArgs()[i]));
      S.compose(Si);
    }
    return S;
  }

  if (A->tag() == Monotype::Tag::Fun && B->tag() == Monotype::Tag::Fun) {
    if (A->funArgs().size() != B->funArgs().size())
      throw UnifyError("arity mismatch");
    Substitution S;
    for (size_t i = 0; i < A->funArgs().size(); ++i) {
      Substitution Si =
          unify(S.apply(A->funArgs()[i]), S.apply(B->funArgs()[i]));
      S.compose(Si);
    }
    Substitution Sr = unify(S.apply(A->funRet()), S.apply(B->funRet()));
    S.compose(Sr);
    return S;
  }

  throw UnifyError("cannot unify given types");
}

} // namespace phi
