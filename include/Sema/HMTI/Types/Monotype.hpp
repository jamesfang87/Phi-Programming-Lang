#pragma once

#include <memory>
#include <optional>
#include <sstream>
#include <string>
#include <unordered_set>
#include <utility>
#include <variant>
#include <vector>

#include "AST/Type.hpp"
#include "Sema/HMTI/Types/MonotypeAtoms.hpp"

namespace phi {

// ---------------------------
// Monotype (shared variant)
// ---------------------------
class Monotype {
private:
  using Variant = std::variant<TypeVar, TypeCon, TypeFun>;
  std::shared_ptr<Variant> ptr;

public:
  // Ctors
  Monotype() = default;
  explicit Monotype(TypeVar v) : ptr(std::make_shared<Variant>(std::move(v))) {}
  explicit Monotype(TypeCon c) : ptr(std::make_shared<Variant>(std::move(c))) {}
  explicit Monotype(TypeFun f) : ptr(std::make_shared<Variant>(std::move(f))) {}

  // Factories
  static Monotype makeVar(int Id) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::nullopt});
  }
  static Monotype makeVar(int Id, std::vector<std::string> Constraints) {
    return Monotype(TypeVar{.Id = Id, .Constraints = Constraints});
  }

  static Monotype makeCon(std::string name, std::vector<Monotype> args = {}) {
    return Monotype(TypeCon{.name = std::move(name), .args = std::move(args)});
  }
  // Accepts a Monotype ret (by value) and wraps it in a shared_ptr internally.
  static Monotype makeFun(std::vector<Monotype> params, Monotype ret) {
    TypeFun f;
    f.params = std::move(params);
    // wrap the provided Monotype into a shared_ptr
    f.ret = std::make_shared<Monotype>(std::move(ret));
    return Monotype(std::move(f));
  }

  static Monotype makeFun(std::vector<Monotype> params,
                          std::shared_ptr<Monotype> ret) {
    return Monotype(TypeFun({std::move(params), std::move(ret)}));
  }

  // Kind checks
  bool isVar() const { return std::holds_alternative<TypeVar>(*ptr); }
  bool isCon() const { return std::holds_alternative<TypeCon>(*ptr); }
  bool isFun() const { return std::holds_alternative<TypeFun>(*ptr); }

  // Accessors
  TypeVar &asVar() { return std::get<TypeVar>(*ptr); }
  const TypeVar &asVar() const { return std::get<TypeVar>(*ptr); }
  TypeCon &asCon() { return std::get<TypeCon>(*ptr); }
  const TypeCon &asCon() const { return std::get<TypeCon>(*ptr); }
  TypeFun &asFun() { return std::get<TypeFun>(*ptr); }
  const TypeFun &asFun() const { return std::get<TypeFun>(*ptr); }

  // Visitor helper
  template <typename VarF, typename ConF, typename FunF>
  auto visit(VarF &&vf, ConF &&cf, FunF &&ff) {
    return std::visit(
        [&](auto &&val) -> decltype(auto) {
          using T = std::decay_t<decltype(val)>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return vf(val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return cf(val);
          else
            return ff(val);
        },
        *ptr);
  }

  template <typename VarF, typename ConF, typename FunF>
  auto visit(VarF &&vf, ConF &&cf, FunF &&ff) const {
    return std::visit(
        [&](auto &&val) -> decltype(auto) {
          using T = std::decay_t<decltype(val)>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return vf(val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return cf(val);
          else
            return ff(val);
        },
        *ptr);
  }

  std::unordered_set<TypeVar> freeTypeVars() const {
    return this->visit(
        [](const TypeVar &v) { return std::unordered_set<TypeVar>{v}; },
        [](const TypeCon &c) {
          std::unordered_set<TypeVar> acc;
          for (auto &a : c.args) {
            auto s = a.freeTypeVars();
            acc.insert(s.begin(), s.end());
          }
          return acc;
        },
        [](const TypeFun &f) {
          std::unordered_set<TypeVar> acc;
          for (auto &p : f.params) {
            auto s = p.freeTypeVars();
            acc.insert(s.begin(), s.end());
          }
          auto r = f.ret->freeTypeVars();
          acc.insert(r.begin(), r.end());
          return acc;
        });
  }

  // Pretty
  std::string toString() const {
    return visit([](const TypeVar &v) { return std::to_string(v.Id); },
                 [](const TypeCon &c) {
                   if (c.args.empty())
                     return c.name;
                   std::ostringstream os;
                   os << c.name << "<";
                   for (size_t i = 0; i < c.args.size(); ++i) {
                     os << c.args[i].toString();
                     if (i + 1 < c.args.size())
                       os << ", ";
                   }
                   os << ">";
                   return os.str();
                 },
                 [](const TypeFun &f) {
                   std::ostringstream os;
                   os << "(";
                   for (size_t i = 0; i < f.params.size(); ++i) {
                     os << f.params[i].toString();
                     if (i + 1 < f.params.size())
                       os << ", ";
                   }
                   os << ") -> " << f.ret->toString();
                   return os.str();
                 });
  }

  // Helpers (simple predicates)
  bool isIntType() const {
    return isCon() && (asCon().name == "i8" || asCon().name == "i16" ||
                       asCon().name == "i32" || asCon().name == "i64");
  }
  bool isFloatType() const {
    return isCon() && (asCon().name == "f32" || asCon().name == "f64");
  }

  [[nodiscard]] Type toAstType() const;
};

} // namespace phi
