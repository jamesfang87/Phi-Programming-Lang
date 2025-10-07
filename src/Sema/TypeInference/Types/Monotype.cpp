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
      const auto &Name = Con.Name;
      if (Name == "i8")
        return Type::makePrimitive(PrimitiveKind::I8, Self->Location);
      if (Name == "i16")
        return Type::makePrimitive(PrimitiveKind::I16, Self->Location);
      if (Name == "i32")
        return Type::makePrimitive(PrimitiveKind::I32, Self->Location);
      if (Name == "i64")
        return Type::makePrimitive(PrimitiveKind::I64, Self->Location);
      if (Name == "u8")
        return Type::makePrimitive(PrimitiveKind::U8, Self->Location);
      if (Name == "u16")
        return Type::makePrimitive(PrimitiveKind::U16, Self->Location);
      if (Name == "u32")
        return Type::makePrimitive(PrimitiveKind::U32, Self->Location);
      if (Name == "u64")
        return Type::makePrimitive(PrimitiveKind::U64, Self->Location);
      if (Name == "f32")
        return Type::makePrimitive(PrimitiveKind::F32, Self->Location);
      if (Name == "f64")
        return Type::makePrimitive(PrimitiveKind::F64, Self->Location);
      if (Name == "string")
        return Type::makePrimitive(PrimitiveKind::String, Self->Location);
      if (Name == "char")
        return Type::makePrimitive(PrimitiveKind::Char, Self->Location);
      if (Name == "bool")
        return Type::makePrimitive(PrimitiveKind::Bool, Self->Location);
      if (Name == "range")
        return Type::makePrimitive(PrimitiveKind::Range, Self->Location);
      if (Name == "null")
        return Type::makePrimitive(PrimitiveKind::Null, Self->Location);

      // Otherwise treat as custom/struct name
      return Type::makeCustom(Name, Self->Location);
    }

    Type operator()(const TypeApp &App) const {
      if (App.Name == "Ptr") {
        assert(App.Args.size() == 1);
        return Type::makePointer(App.Args.front().toAstType(), Self->Location);
      }
      if (App.Name == "Ref") {
        assert(App.Args.size() == 1);
        return Type::makeReference(App.Args.front().toAstType(),
                                   Self->Location);
      }
      if (App.Name == "Tuple") {
        std::vector<Type> TypeArgs;
        TypeArgs.reserve(App.Args.size());
        for (const auto &Arg : App.Args)
          TypeArgs.push_back(Arg.toAstType());
        return Type::makeTuple(TypeArgs, Self->Location);
      }
      std::vector<Type> TypeArgs;
      TypeArgs.reserve(App.Args.size());
      for (const auto &Arg : App.Args)
        TypeArgs.push_back(Arg.toAstType());
      return Type::makeGeneric(App.Name, TypeArgs, Self->Location);
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
  // If Monotype *is* a variant: return std::visit(Visitor{this}, *this);
}

std::string Monotype::toString() const {
  struct Visitor {
    std::string operator()(const TypeVar &Var) const {
      return std::to_string(Var.Id);
    }

    std::string operator()(const TypeCon &Con) const {
      if (Con.Args.empty())
        return Con.Name;
      std::ostringstream Os;
      Os << Con.Name << "<";
      for (size_t I = 0; I < Con.Args.size(); ++I) {
        Os << Con.Args[I].toString();
        if (I + 1 < Con.Args.size())
          Os << ", ";
      }
      Os << ">";
      return Os.str();
    }

    std::string operator()(const TypeApp &App) const {
      if (App.Args.empty())
        return App.Name;
      std::ostringstream Os;
      Os << App.Name << "<";
      for (size_t I = 0; I < App.Args.size(); ++I) {
        Os << App.Args[I].toString();
        if (I + 1 < App.Args.size())
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
      std::unordered_set<TypeVar> AllFTVs;
      for (const auto &Arg : Con.Args) {
        auto FTV = Arg.freeTypeVars();
        AllFTVs.insert(FTV.begin(), FTV.end());
      }
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
