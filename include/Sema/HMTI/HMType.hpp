#pragma once

#include <cstdint>
#include <memory>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

namespace phi {

// Forward
class TypeEnv;

// Type variable
struct TypeVar {
  int Id;
  bool operator==(const TypeVar &O) const { return Id == O.Id; }
};
struct TypeVarHash {
  size_t operator()(const TypeVar &V) const noexcept {
    return std::hash<int>{}(V.Id);
  }
};

// Monotype: Var | Con(name, args...) | Fun(args..., ret)
class Monotype {
public:
  enum class Kind : uint8_t { Var, Con, Fun };

  static std::shared_ptr<Monotype> var(TypeVar V);
  static std::shared_ptr<Monotype>
  con(std::string Name, std::vector<std::shared_ptr<Monotype>> Args = {});
  static std::shared_ptr<Monotype>
  fun(std::vector<std::shared_ptr<Monotype>> Params,
      std::shared_ptr<Monotype> Ret);

  Kind tag() const noexcept { return K; }
  const TypeVar &asVar() const { return V; }
  std::pair<std::vector<std::shared_ptr<Monotype>>, std::shared_ptr<Monotype>>
  asFun() const {
    return make_pair(FunArgs, FunRet);
  }

  const std::string &getConName() const { return ConName; }
  const std::vector<std::shared_ptr<Monotype>> &getConArgs() const {
    return ConArgs;
  }
  const std::vector<std::shared_ptr<Monotype>> &getFunArgs() const {
    return FunArgs;
  }
  const std::shared_ptr<Monotype> &getFunReturn() const { return FunRet; }

  void setFun(std::vector<std::shared_ptr<Monotype>> Params,
              std::shared_ptr<Monotype> Ret);

private:
  Monotype() = default;
  Kind K = Kind::Con;
  TypeVar V{-1};
  std::string ConName;
  std::vector<std::shared_ptr<Monotype>> ConArgs;
  std::vector<std::shared_ptr<Monotype>> FunArgs;
  std::shared_ptr<Monotype> FunRet;
};

// Polytype (Scheme): forall quant. body
class Polytype {
public:
  Polytype() = default;
  Polytype(std::vector<TypeVar> Quant, std::shared_ptr<Monotype> Body)
      : Quant(std::move(Quant)), Body(std::move(Body)) {}
  const std::vector<TypeVar> &getQuant() const { return Quant; }
  const std::shared_ptr<Monotype> &getBody() const { return Body; }
  void setBody(std::shared_ptr<Monotype> B) { Body = std::move(B); }
  void setQuant(std::vector<TypeVar> Q) { Quant = std::move(Q); }

private:
  std::vector<TypeVar> Quant;
  std::shared_ptr<Monotype> Body;
};

// Substitution
class Substitution {
public:
  std::unordered_map<TypeVar, std::shared_ptr<Monotype>, TypeVarHash> Map;
  std::shared_ptr<Monotype> apply(const std::shared_ptr<Monotype> &T) const;
  Polytype apply(const Polytype &S) const;
  void compose(const Substitution &S2); // this := S2 ∘ this
  bool empty() const { return Map.empty(); }
};

// helpers
std::unordered_set<TypeVar, TypeVarHash>
freeTypeVars(const std::shared_ptr<Monotype> &T);
std::unordered_set<TypeVar, TypeVarHash> freeTypeVars(const Polytype &S);
std::unordered_set<TypeVar, TypeVarHash> freeTypeVars(const TypeEnv &Env);

// TypeVar factory
class TypeVarFactory {
  int Next = 0;

public:
  TypeVar fresh() { return TypeVar{Next++}; }
};

// instantiate / generalize
std::shared_ptr<Monotype> instantiate(const Polytype &S, TypeVarFactory &F);
Polytype generalize(const TypeEnv &Env, const std::shared_ptr<Monotype> &T);

// unify with occurs-check
class UnifyError : public std::runtime_error {
public:
  using std::runtime_error::runtime_error;
};
Substitution unify(const std::shared_ptr<Monotype> &A,
                   const std::shared_ptr<Monotype> &B);

// canonical base constructors (names chosen to map to your AST Type)
inline std::shared_ptr<Monotype> makeI32() { return Monotype::con("i32"); }
inline std::shared_ptr<Monotype> makeI64() { return Monotype::con("i64"); }
inline std::shared_ptr<Monotype> makeF32() { return Monotype::con("f32"); }
inline std::shared_ptr<Monotype> makeF64() { return Monotype::con("f64"); }
inline std::shared_ptr<Monotype> makeBool() { return Monotype::con("bool"); }
inline std::shared_ptr<Monotype> makeChar() { return Monotype::con("char"); }
inline std::shared_ptr<Monotype> makeString() {
  return Monotype::con("string");
}
inline std::shared_ptr<Monotype> makeUnit() { return Monotype::con("unit"); }
inline std::shared_ptr<Monotype> makeRange(std::shared_ptr<Monotype> T) {
  return Monotype::con("range", {std::move(T)});
}
inline std::shared_ptr<Monotype> makeStruct(std::string Name) {
  return Monotype::con(std::move(Name));
}

} // namespace phi
