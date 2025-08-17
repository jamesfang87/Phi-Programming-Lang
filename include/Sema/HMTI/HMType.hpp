#pragma once

#include <memory>
#include <string>
#include <unordered_map>
#include <unordered_set>
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
  enum class Tag { Var, Con, Fun };

  static std::shared_ptr<Monotype> var(TypeVar V);
  static std::shared_ptr<Monotype>
  con(std::string Name, std::vector<std::shared_ptr<Monotype>> Args = {});
  static std::shared_ptr<Monotype>
  fun(std::vector<std::shared_ptr<Monotype>> Params,
      std::shared_ptr<Monotype> Ret);

  Tag tag() const noexcept { return Tag_; }
  const TypeVar &asVar() const { return V_; }
  const std::string &conName() const { return ConName_; }
  const std::vector<std::shared_ptr<Monotype>> &conArgs() const {
    return ConArgs_;
  }
  const std::vector<std::shared_ptr<Monotype>> &funArgs() const {
    return FunArgs_;
  }
  const std::shared_ptr<Monotype> &funRet() const { return FunRet_; }

  void setFun(std::vector<std::shared_ptr<Monotype>> Params,
              std::shared_ptr<Monotype> Ret);

private:
  Monotype() = default;
  Tag Tag_ = Tag::Con;
  TypeVar V_{-1};
  std::string ConName_;
  std::vector<std::shared_ptr<Monotype>> ConArgs_;
  std::vector<std::shared_ptr<Monotype>> FunArgs_;
  std::shared_ptr<Monotype> FunRet_;
};

// Polytype (Scheme): forall quant. body
class Polytype {
public:
  Polytype() = default;
  Polytype(std::vector<TypeVar> Quant, std::shared_ptr<Monotype> Body)
      : Quant_(std::move(Quant)), Body_(std::move(Body)) {}
  const std::vector<TypeVar> &quant() const { return Quant_; }
  const std::shared_ptr<Monotype> &body() const { return Body_; }
  void setBody(std::shared_ptr<Monotype> B) { Body_ = std::move(B); }
  void setQuant(std::vector<TypeVar> Q) { Quant_ = std::move(Q); }

private:
  std::vector<TypeVar> Quant_;
  std::shared_ptr<Monotype> Body_;
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
