#pragma once

#include <cassert>
#include <memory>
#include <optional>
#include <string>
#include <unordered_set>
#include <utility>
#include <variant>
#include <vector>

#include "AST/TypeSystem/Type.hpp:w"
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
 * - TypeCon: Type constants (e.g., Int, Bool)
 * - TypeApp: Type applications (e.g., List[Int])
 * - TypeFun: Function types (e.g., Int -> Bool)
 */
class Monotype {
public:
  using Variant = std::variant<TypeVar, TypeCon, TypeApp, TypeFun>;
  //===--------------------------------------------------------------------===//
  // Constructors & Destructors
  //===--------------------------------------------------------------------===//

  // Default constructor
  Monotype() = default;

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

  [[nodiscard]] Variant getPtr() const { return *Ptr; }
  [[nodiscard]] SrcLocation getLocation() const { return Location; }

  //===--------------------------------------------------------------------===//
  // Factory Methods
  //===--------------------------------------------------------------------===//

  /// Creates a type variable monotype
  static Monotype makeVar(const int Id,
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::nullopt},
                    std::move(L));
  }

  /// Creates a constrained type variable monotype
  static Monotype makeVar(const int Id, std::vector<std::string> Constraints,
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(TypeVar{.Id = Id, .Constraints = std::move(Constraints)},
                    std::move(L));
  }

  static Monotype makeVar(const TypeVar &V) { return makeVar(V.Id); }

  /// Creates a type application monotype
  static Monotype makeApp(std::string Name, std::vector<Monotype> Args = {},
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(
        TypeApp{.AppKind = TypeApp::CustomKind{.Id = std::move(Name)},
                .Args = std::move(Args)},
        std::move(L));
  }

  static Monotype makeApp(BuiltinTy::Kind Builtin,
                          std::vector<Monotype> Args = {},
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(TypeApp{.AppKind = Builtin, .Args = std::move(Args)},
                    std::move(L));
  }

  static Monotype
  makeApp(std::variant<BuiltinTy::Kind, TypeApp::CustomKind> Kind,
          std::vector<Monotype> Args = {},
          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(
        TypeApp{.AppKind = std::move(Kind), .Args = std::move(Args)},
        std::move(L));
  }

  static Monotype makeCon(BuiltinTy::Kind Primitive,
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return Monotype(TypeCon{.Data = Primitive,
                            .StringRep = BuiltinTy::KindToString(Primitive)},
                    std::move(L));
  }

  static Monotype makeCon(std::string Name, StructDecl *D, SrcLocation L) {
    assert(D && "Passed in nullptr for StructDecl ptr");
    return Monotype(TypeCon{.Data = TypeCon::StructType{.Id = Name, .D = D},
                            .StringRep = std::move(Name)},
                    std::move(L));
  }

  static Monotype makeCon(std::string Name, EnumDecl *D, SrcLocation L) {
    assert(D && "Passed in nullptr for EnumDecl ptr");
    return Monotype(TypeCon{.Data = TypeCon::EnumType{.Id = Name, .D = D},
                            .StringRep = std::move(Name)},
                    std::move(L));
  }

  /// Creates a function type monotype
  static Monotype makeFun(std::vector<Monotype> Params, Monotype Ret,
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    TypeFun F;
    F.Params = std::move(Params);
    // wrap the provided Monotype into a shared_ptr
    F.Ret = std::make_shared<Monotype>(std::move(Ret));
    return {std::move(F), std::move(L)};
  }

  /// Creates a function type monotype from shared pointer
  static Monotype makeFun(std::vector<Monotype> Params,
                          const std::shared_ptr<Monotype> &Ret,
                          SrcLocation L = {.Path = "", .Line = -1, .Col = -1}) {
    return makeFun(std::move(Params), *Ret, std::move(L));
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
  // Generalize
  //===--------------------------------------------------------------------===//

  class Polytype generalize(const class TypeEnv &Env);

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
    if (!isCon())
      return false;

    if (auto *Prim = std::get_if<BuiltinTy::Kind>(&this->asCon().Data)) {
      switch (*Prim) {
      case BuiltinTy::Kind::i8:
      case BuiltinTy::Kind::i16:
      case BuiltinTy::Kind::i32:
      case BuiltinTy::Kind::i64:
      case BuiltinTy::Kind::u8:
      case BuiltinTy::Kind::u16:
      case BuiltinTy::Kind::u32:
      case BuiltinTy::Kind::u64:
        return true;
      default:
        return false;
      }
    }
    return false;
  }

  [[nodiscard]] bool isFloatType() const {
    if (!isCon())
      return false;

    if (auto *Prim = std::get_if<BuiltinTy::Kind>(&this->asCon().Data)) {
      switch (*Prim) {
      case BuiltinTy::Kind::f32:
      case BuiltinTy::Kind::f64:
        return true;
      default:
        return false;
      }
    }
    return false;
  }

  [[nodiscard]] bool sameMonotypeKind(const Monotype &Other) const;

  bool operator==(const Monotype &Rhs) const { return *Ptr == Rhs.getPtr(); }
  bool operator!=(const Monotype &Rhs) const { return !(*this == Rhs); }

private:
  //===--------------------------------------------------------------------===//
  // Member Variables
  //===--------------------------------------------------------------------===//

  std::shared_ptr<Variant> Ptr;
  SrcLocation Location{.Path = "", .Line = -1, .Col = -1};
};

} // namespace phi
