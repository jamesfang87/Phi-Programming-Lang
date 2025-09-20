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
#include "SrcManager/SrcLocation.hpp"

namespace phi {

//===----------------------------------------------------------------------===//
// Monotype - Shared variant representing monomorphic types in HM system
//===----------------------------------------------------------------------===//

/**
 * @brief Monomorphic type representation for Hindley-Milner type inference
 *
 * A monotype is a type without universal quantifiers (no type variables
 * that are polymorphic). This class provides a shared variant container
 * for the four fundamental monotype constructs:
 * - TypeVar: Type variables (e.g., 'a, 'b)
 * - TypeCon: Type constructors (e.g., Int, Bool)
 * - TypeApp: Type applications (e.g., List[Int])
 * - TypeFun: Function types (e.g., Int -> Bool)
 */
class Monotype {
public:
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  // Default constructor
  Monotype() = default;

  // Basic variant constructors
  explicit Monotype(TypeVar V) : Ptr(std::make_shared<Variant>(std::move(V))) {}
  explicit Monotype(TypeCon C) : Ptr(std::make_shared<Variant>(std::move(C))) {}
  explicit Monotype(TypeApp A) : Ptr(std::make_shared<Variant>(std::move(A))) {}
  explicit Monotype(TypeFun F) : Ptr(std::make_shared<Variant>(std::move(F))) {}

  // Constructors with source location
  Monotype(TypeVar V, SrcLocation L)
      : Ptr(std::make_shared<Variant>(std::move(V))), Location(std::move(L)) {}
  Monotype(TypeCon C, SrcLocation L)
      : Ptr(std::make_shared<Variant>(std::move(C))), Location(std::move(L)) {}
  Monotype(TypeApp A, SrcLocation L)
      : Ptr(std::make_shared<Variant>(std::move(A))), Location(std::move(L)) {}
  Monotype(TypeFun F, SrcLocation L)
      : Ptr(std::make_shared<Variant>(std::move(F))), Location(std::move(L)) {}

  //===--------------------------------------------------------------------===//
  // Getters
  //===--------------------------------------------------------------------===//

  [[nodiscard]] SrcLocation getLocation() const { return Location; }

  //===--------------------------------------------------------------------===//
  // Factory Methods
  //===--------------------------------------------------------------------===//

  /// Creates a type variable monotype
  static Monotype makeVar(const int Id, SrcLocation L = {"", -1, -1}) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::nullopt},
                    std::move(L));
  }

  /// Creates a constrained type variable monotype
  static Monotype makeVar(const int Id, std::vector<std::string> Constraints,
                          SrcLocation L = {"", -1, -1}) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::move(Constraints)},
                    std::move(L));
  }

  static Monotype makeVar(const TypeVar &V) { return makeVar(V.Id); }

  /// Creates a type application monotype
  static Monotype makeApp(std::string Name, std::vector<Monotype> Args = {},
                          SrcLocation L = {"", -1, -1}) {
    return Monotype(TypeApp{.Name = std::move(Name), .Args = std::move(Args)},
                    std::move(L));
  }

  /// Creates a type constructor monotype
  static Monotype makeCon(std::string Name, std::vector<Monotype> Args = {},
                          SrcLocation L = {"", -1, -1}) {
    return Monotype(TypeCon{.Name = std::move(Name), .Args = std::move(Args)},
                    std::move(L));
  }

  /// Creates a function type monotype
  static Monotype makeFun(std::vector<Monotype> Params, Monotype Ret,
                          SrcLocation L = {"", -1, -1}) {
    TypeFun F;
    F.Params = std::move(Params);
    // wrap the provided Monotype into a shared_ptr
    F.Ret = std::make_shared<Monotype>(std::move(Ret));
    return Monotype(std::move(F), std::move(L));
  }

  /// Creates a function type monotype from shared pointer
  static Monotype makeFun(std::vector<Monotype> params,
                          const std::shared_ptr<Monotype> &ret,
                          SrcLocation L = {"", -1, -1}) {
    return makeFun(std::move(params), *ret, std::move(L));
  }

  //===--------------------------------------------------------------------===//
  // Type Kind Predicates
  //===--------------------------------------------------------------------===//

  [[nodiscard]] bool isVar() const {
    return std::holds_alternative<TypeVar>(*Ptr);
  }
  [[nodiscard]] bool isCon() const {
    return std::holds_alternative<TypeCon>(*Ptr);
  }
  [[nodiscard]] bool isApp() const {
    return std::holds_alternative<TypeApp>(*Ptr);
  }
  [[nodiscard]] bool isFun() const {
    return std::holds_alternative<TypeFun>(*Ptr);
  }

  //===--------------------------------------------------------------------===//
  // Variant Accessors
  //===--------------------------------------------------------------------===//

  TypeVar &asVar() { return std::get<TypeVar>(*Ptr); }
  TypeCon &asCon() { return std::get<TypeCon>(*Ptr); }
  TypeApp &asApp() { return std::get<TypeApp>(*Ptr); }
  TypeFun &asFun() { return std::get<TypeFun>(*Ptr); }

  [[nodiscard]] const TypeVar &asVar() const { return std::get<TypeVar>(*Ptr); }
  [[nodiscard]] const TypeCon &asCon() const { return std::get<TypeCon>(*Ptr); }
  [[nodiscard]] const TypeApp &asApp() const { return std::get<TypeApp>(*Ptr); }
  [[nodiscard]] const TypeFun &asFun() const { return std::get<TypeFun>(*Ptr); }

  //===--------------------------------------------------------------------===//
  // Visitor Pattern Support
  //===--------------------------------------------------------------------===//

  template <typename VarT, typename ConT, typename AppT, typename FunT>
  auto visit(VarT &&Var, ConT &&Con, AppT App, FunT &&Fun) {
    return std::visit(
        [&]<typename Input>(Input &&Val) -> decltype(auto) {
          using T = std::decay_t<Input>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return Var(Val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return Con(Val);
          else if constexpr (std::is_same_v<T, TypeApp>)
            return App(Val);
          else
            return Fun(Val);
        },
        *Ptr);
  }

  template <typename VarT, typename ConT, typename AppT, typename FunT>
  auto visit(VarT &&Var, ConT &&Con, AppT App, FunT &&Fun) const {
    return std::visit(
        [&]<typename Input>(Input &&Val) -> decltype(auto) {
          using T = std::decay_t<Input>;
          if constexpr (std::is_same_v<T, TypeVar>)
            return Var(Val);
          else if constexpr (std::is_same_v<T, TypeCon>)
            return Con(Val);
          else if constexpr (std::is_same_v<T, TypeApp>)
            return App(Val);
          else
            return Fun(Val);
        },
        *Ptr);
  }

  //===--------------------------------------------------------------------===//
  // Conversion & Analysis Methods
  //===--------------------------------------------------------------------===//

  /// Converts HM monotype to AST type representation
  [[nodiscard]] Type toAstType() const;

  /// Extracts all free type variables in this monotype
  [[nodiscard]] std::unordered_set<TypeVar> freeTypeVars() const;

  /// Generates string representation for debugging/display
  [[nodiscard]] std::string toString() const;

  //===--------------------------------------------------------------------===//
  // Type Classification Helpers
  //===--------------------------------------------------------------------===//

  /// Checks if this monotype represents an integer type
  [[nodiscard]] bool isIntType() const {
    return isCon() && (asCon().Name == "i8" || asCon().Name == "i16" ||
                       asCon().Name == "i32" || asCon().Name == "i64" ||
                       asCon().Name == "u8" || asCon().Name == "u16" ||
                       asCon().Name == "u32" || asCon().Name == "u64");
  }

  /// Checks if this monotype represents a floating-point type
  [[nodiscard]] bool isFloatType() const {
    return isCon() && (asCon().Name == "f32" || asCon().Name == "f64");
  }

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  using Variant = std::variant<TypeVar, TypeCon, TypeApp, TypeFun>;
  std::shared_ptr<Variant> Ptr;
  SrcLocation Location{"", -1, -1};
};

} // namespace phi
