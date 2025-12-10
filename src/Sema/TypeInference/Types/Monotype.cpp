#include "Sema/TypeInference/Types/Monotype.hpp"

#include <cassert>
#include <sstream>
#include <string>
#include <unordered_set>
#include <variant>
#include <vector>

#include "AST/Type.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"

namespace phi {

Polytype Monotype::generalize(const TypeEnv &Env) {
  const auto MonotypeFTVs = this->freeTypeVars();
  const auto EnvFTVs = Env.freeTypeVars();

  // Quant = ftv(t) \ ftv(env)
  std::vector<TypeVar> Quant;
  for (auto &TypeVar : MonotypeFTVs)
    if (!EnvFTVs.contains(TypeVar))
      Quant.push_back(TypeVar);

  return {std::move(Quant), *this};
}

Type Monotype::toAstType() const {
  struct Visitor {
    const Monotype *Self;

    Type operator()(const TypeVar &Var) const {
      (void)Var;
      assert(false && "Cannot convert Monotype which has unknown type to a "
                      "concrete AST/Type");
      return Type::makePrimitive(PrimitiveKind::Null, Self->Location);
    }

    Type operator()(const TypeCon &Con) const {
      return std::visit(
          [&](auto &&Val) -> Type {
            using T = std::decay_t<decltype(Val)>;

            if constexpr (std::is_same_v<T, PrimitiveKind>) {
              return Type::makePrimitive(Val, Self->Location);
            } else if constexpr (std::is_same_v<T, TypeCon::StructType>) {
              return Type::makeStruct(Val.Id, Self->Location);
            } else if constexpr (std::is_same_v<T, TypeCon::EnumType>) {
              return Type::makeEnum(Val.Id, Self->Location);
            }
          },
          Con.Data);
    }

    Type operator()(const TypeApp &App) const {
      std::vector<Type> TypeArgs;
      TypeArgs.reserve(App.Args.size());
      for (const auto &Arg : App.Args)
        TypeArgs.push_back(Arg.toAstType());

      return std::visit(
          [&](auto &&K) -> Type {
            using T = std::decay_t<decltype(K)>;
            if constexpr (std::is_same_v<T, TypeApp::BuiltinKind>) {
              if (K == TypeApp::BuiltinKind::Ptr)
                return Type::makePointer(TypeArgs.at(0), Self->Location);
              if (K == TypeApp::BuiltinKind::Ref)
                return Type::makeReference(TypeArgs.at(0), Self->Location);
              if (K == TypeApp::BuiltinKind::Range)
                return Type::makePrimitive(PrimitiveKind::Range,
                                           Self->Location);
              if (K == TypeApp::BuiltinKind::Tuple)
                return Type::makeTuple(TypeArgs, Self->Location);
              throw std::runtime_error("Unknown BuiltinKind");
            } else {
              return Type::makeGeneric(K.Id, TypeArgs, Self->Location);
            }
          },
          App.AppKind);
    }

    Type operator()(const TypeFun &Fun) const {
      std::vector<Type> ParamTypes;
      ParamTypes.reserve(Fun.Params.size());
      for (const auto &T : Fun.Params)
        ParamTypes.push_back(T.toAstType());
      return Type::makeFunction(ParamTypes, Fun.Ret->toAstType(),
                                Self->Location);
    }
  };

  return std::visit(Visitor{this}, *this->Ptr);
}

std::string Monotype::toString() const {
  struct Visitor {
    std::string operator()(const TypeVar &Var) const {
      return std::to_string(Var.Id);
    }

    std::string operator()(const TypeCon &Con) const { return Con.StringRep; }

    std::string operator()(const TypeApp &App) const {
      // Helper to get the base name
      auto name = std::visit(
          [](auto &&K) -> std::string {
            using T = std::decay_t<decltype(K)>;
            if constexpr (std::is_same_v<T, TypeApp::BuiltinKind>) {
              switch (K) {
              case TypeApp::BuiltinKind::Ref:
                return "Ref";
              case TypeApp::BuiltinKind::Ptr:
                return "Ptr";
              case TypeApp::BuiltinKind::Tuple:
                return "Tuple";
              case TypeApp::BuiltinKind::Range:
                return "Range";
              default:
                return "Unknown";
              }
            } else if constexpr (std::is_same_v<T, TypeApp::CustomKind>) {
              return K.Id;
            }
          },
          App.AppKind);

      // Append type arguments if any
      if (App.Args.empty())
        return name;

      std::ostringstream Os;
      Os << name << "<";
      for (size_t i = 0; i < App.Args.size(); ++i) {
        Os << App.Args[i].toString();
        if (i + 1 < App.Args.size())
          Os << ", ";
      }
      Os << ">";
      return Os.str();
    }

    std::string operator()(const TypeFun &Fun) const {
      std::ostringstream Os;
      Os << "(";
      for (size_t I = 0; I < Fun.Params.size(); ++I) {
        Os << Fun.Params[I].toString();
        if (I + 1 < Fun.Params.size())
          Os << ", ";
      }
      Os << ") -> " << Fun.Ret->toString();
      return Os.str();
    }
  };

  return std::visit(Visitor{}, *this->Ptr);
}

std::unordered_set<TypeVar> Monotype::freeTypeVars() const {
  struct Visitor {
    std::unordered_set<TypeVar> operator()(const TypeVar &Var) const {
      return std::unordered_set<TypeVar>{Var};
    }

    std::unordered_set<TypeVar> operator()(const TypeCon &Con) const {
      (void)Con;
      std::unordered_set<TypeVar> AllFTVs;
      return AllFTVs;
    }

    std::unordered_set<TypeVar> operator()(const TypeApp &App) const {
      std::unordered_set<TypeVar> AllFTVs;
      for (const auto &Arg : App.Args) {
        auto FTV = Arg.freeTypeVars();
        AllFTVs.insert(FTV.begin(), FTV.end());
      }
      return AllFTVs;
    }

    std::unordered_set<TypeVar> operator()(const TypeFun &Fun) const {
      std::unordered_set<TypeVar> AllFTVs;
      for (const auto &Params : Fun.Params) {
        auto FTV = Params.freeTypeVars();
        AllFTVs.insert(FTV.begin(), FTV.end());
      }
      auto Ret = Fun.Ret->freeTypeVars();
      AllFTVs.insert(Ret.begin(), Ret.end());
      return AllFTVs;
    }
  };

  return std::visit(Visitor{}, *this->Ptr);
}

bool Monotype::sameMonotypeKind(const Monotype &Other) const {
  return std::visit(
      [&](const auto &SelfInner) -> bool {
        using T = std::decay_t<decltype(SelfInner)>;
        return std::holds_alternative<T>(*Other.Ptr);
      },
      *Ptr);
}

} // namespace phi
