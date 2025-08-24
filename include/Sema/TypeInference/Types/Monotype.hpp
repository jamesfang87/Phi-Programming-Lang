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
#include "Sema/TypeInference/Types/MonotypeAtoms.hpp"

namespace phi {

// ---------------------------
// Monotype (shared variant)
// ---------------------------
class Monotype {
private:
  using Variant = std::variant<TypeVar, TypeCon, TypeFun>;
  std::shared_ptr<Variant> Ptr;

public:
  // Ctors
  Monotype() = default;
  explicit Monotype(TypeVar v) : Ptr(std::make_shared<Variant>(std::move(v))) {}
  explicit Monotype(TypeCon c) : Ptr(std::make_shared<Variant>(std::move(c))) {}
  explicit Monotype(TypeFun f) : Ptr(std::make_shared<Variant>(std::move(f))) {}

  // Factories
  static Monotype makeVar(const int Id) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::nullopt});
  }
  static Monotype makeVar(const int Id, std::vector<std::string> Constraints) {
    return Monotype(TypeVar{.Id = Id, .Constraints = Constraints});
  }

  static Monotype makeCon(std::string Name, std::vector<Monotype> Args = {}) {
    return Monotype(TypeCon{.Name = std::move(Name), .Args = std::move(Args)});
  }
  // Accepts a Monotype ret (by value) and wraps it in a shared_ptr internally.
  static Monotype makeFun(std::vector<Monotype> Params, Monotype Ret) {
    TypeFun F;
    F.Params = std::move(Params);
    // wrap the provided Monotype into a shared_ptr
    F.Ret = std::make_shared<Monotype>(std::move(Ret));
    return Monotype(std::move(F));
  }

  static Monotype makeFun(std::vector<Monotype> params,
                          const std::shared_ptr<Monotype> &ret) {
    return makeFun(std::move(params), *ret);
  }

  // Kind checks
  [[nodiscard]] bool isVar() const { return std::holds_alternative<TypeVar>(*Ptr); }
  [[nodiscard]] bool isCon() const { return std::holds_alternative<TypeCon>(*Ptr); }
  [[nodiscard]] bool isFun() const { return std::holds_alternative<TypeFun>(*Ptr); }

  // Accessors
  TypeVar &asVar() { return std::get<TypeVar>(*Ptr); }
  TypeCon &asCon() { return std::get<TypeCon>(*Ptr); }
  TypeFun &asFun() { return std::get<TypeFun>(*Ptr); }

  [[nodiscard]] const TypeVar &asVar() const { return std::get<TypeVar>(*Ptr); }
  [[nodiscard]] const TypeCon &asCon() const { return std::get<TypeCon>(*Ptr); }
  [[nodiscard]] const TypeFun &asFun() const { return std::get<TypeFun>(*Ptr); }

  // Visitor helper
  template <typename VarF, typename ConF, typename FunF>
  auto visit(VarF &&vf, ConF &&cf, FunF &&ff) {
    return std::visit(
        [&]<typename Input>(Input &&val) -> decltype(auto) {
          using T = std::decay_t<Input>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return vf(val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return cf(val);
          else
            return ff(val);
        },
        *Ptr);
  }

  template <typename VarF, typename ConF, typename FunF>
  auto visit(VarF &&Var, ConF &&Con, FunF &&Fun) const {
    return std::visit(
        [&]<typename Input>(Input &&Val) -> decltype(auto) {
          using T = std::decay_t<Input>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return Var(Val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return Con(Val);
          else
            return Fun(Val);
        },
        *Ptr);
  }

  [[nodiscard]] std::unordered_set<TypeVar> freeTypeVars() const {
    return this->visit(
        [](const TypeVar &Var) { return std::unordered_set<TypeVar>{Var}; },
        [](const TypeCon &Con) {
          std::unordered_set<TypeVar> acc;
          for (auto &a : Con.Args) {
            auto s = a.freeTypeVars();
            acc.insert(s.begin(), s.end());
          }
          return acc;
        },
        [](const TypeFun &Fun) {
          std::unordered_set<TypeVar> acc;
          for (auto &p : Fun.Params) {
            auto s = p.freeTypeVars();
            acc.insert(s.begin(), s.end());
          }
          auto r = Fun.Ret->freeTypeVars();
          acc.insert(r.begin(), r.end());
          return acc;
        });
  }

  // Pretty
  [[nodiscard]] std::string toString() const {
    return visit([](const TypeVar &Var) { return std::to_string(Var.Id); },
                 [](const TypeCon &Con) {
                   if (Con.Args.empty())
                     return Con.Name;
                   std::ostringstream os;
                   os << Con.Name << "<";
                   for (size_t i = 0; i < Con.Args.size(); ++i) {
                     os << Con.Args[i].toString();
                     if (i + 1 < Con.Args.size())
                       os << ", ";
                   }
                   os << ">";
                   return os.str();
                 },
                 [](const TypeFun &Fun) {
                   std::ostringstream os;
                   os << "(";
                   for (size_t i = 0; i < Fun.Params.size(); ++i) {
                     os << Fun.Params[i].toString();
                     if (i + 1 < Fun.Params.size())
                       os << ", ";
                   }
                   os << ") -> " << Fun.Ret->toString();
                   return os.str();
                 });
  }

  // Helpers (simple predicates)
  [[nodiscard]] bool isIntType() const {
    return isCon() && (asCon().Name == "i8" || asCon().Name == "i16" ||
                       asCon().Name == "i32" || asCon().Name == "i64");
  }
  [[nodiscard]] bool isFloatType() const {
    return isCon() && (asCon().Name == "f32" || asCon().Name == "f64");
  }

  [[nodiscard]] Type toAstType() const;
};

} // namespace phi
