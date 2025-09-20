#include "Sema/TypeInference/TypeEnv.hpp"

#include <cassert>
#include <print>
#include <string>
#include <vector>

#include "AST/Type.hpp"

namespace phi {

Type Monotype::toAstType() const {
  return this->visit(
      [&](const TypeVar &Var) {
        (void)Var;
        assert(false && "Cannot convert Monotype which has unknown type to a "
                        "concrete AST/Type");
        return Type::makePrimitive(PrimitiveKind::Null, this->Location);
      },
      [&](const TypeCon &Con) {
        const auto &Name = Con.Name;
        if (Name == "i8")
          return Type::makePrimitive(PrimitiveKind::I8, this->Location);
        if (Name == "i16")
          return Type::makePrimitive(PrimitiveKind::I16, this->Location);
        if (Name == "i32")
          return Type::makePrimitive(PrimitiveKind::I32, this->Location);
        if (Name == "i64")
          return Type::makePrimitive(PrimitiveKind::I64, this->Location);
        if (Name == "u8")
          return Type::makePrimitive(PrimitiveKind::U8, this->Location);
        if (Name == "u16")
          return Type::makePrimitive(PrimitiveKind::U16, this->Location);
        if (Name == "u32")
          return Type::makePrimitive(PrimitiveKind::U32, this->Location);
        if (Name == "u64")
          return Type::makePrimitive(PrimitiveKind::U64, this->Location);
        if (Name == "f32")
          return Type::makePrimitive(PrimitiveKind::F32, this->Location);
        if (Name == "f64")
          return Type::makePrimitive(PrimitiveKind::F64, this->Location);
        if (Name == "string")
          return Type::makePrimitive(PrimitiveKind::String, this->Location);
        if (Name == "char")
          return Type::makePrimitive(PrimitiveKind::Char, this->Location);
        if (Name == "bool")
          return Type::makePrimitive(PrimitiveKind::Bool, this->Location);
        if (Name == "range")
          return Type::makePrimitive(PrimitiveKind::Range, this->Location);
        if (Name == "null")
          return Type::makePrimitive(PrimitiveKind::Null, this->Location);

        // Otherwise treat as custom/struct name
        return Type::makeCustom(Name, this->Location);
      },
      [&](const TypeApp &App) {
        if (App.Name == "Ptr") {
          // arity should be 1 exactly
          assert(App.Args.size() == 1);
          return Type::makePointer(App.Args.front().toAstType(),
                                   this->Location);
        }

        if (App.Name == "Ref") {
          // arity should be 1 exactly
          assert(App.Args.size() == 1);
          return Type::makeReference(App.Args.front().toAstType(),
                                     this->Location);
        }

        std::vector<Type> TypeArgs;
        TypeArgs.reserve(App.Args.size());
        for (const auto &Arg : App.Args) {
          TypeArgs.push_back(Arg.toAstType());
        }
        return Type::makeGeneric(App.Name, TypeArgs, this->Location);
      },
      [&](const TypeFun &Fun) {
        std::vector<Type> ParamTypes;
        ParamTypes.reserve(Fun.Params.size());
        for (const auto &T : Fun.Params) {
          ParamTypes.push_back(T.toAstType());
        }
        return Type::makeFunction(ParamTypes, Fun.Ret->toAstType(),
                                  this->Location);
      });
}

std::string Monotype::toString() const {
  return visit([](const TypeVar &Var) { return std::to_string(Var.Id); },
               [](const TypeCon &Con) {
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
               },
               [](const TypeApp &App) {
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
               },
               [](const TypeFun &Fun) {
                 std::ostringstream Os;
                 Os << "(";
                 for (size_t I = 0; I < Fun.Params.size(); ++I) {
                   Os << Fun.Params[I].toString();
                   if (I + 1 < Fun.Params.size())
                     Os << ", ";
                 }
                 Os << ") -> " << Fun.Ret->toString();
                 return Os.str();
               });
}

std::unordered_set<TypeVar> Monotype::freeTypeVars() const {
  return this->visit(
      [](const TypeVar &Var) { return std::unordered_set<TypeVar>{Var}; },
      [](const TypeCon &Con) {
        std::unordered_set<TypeVar> AllFTVs;
        for (auto &Arg : Con.Args) {
          auto FTV = Arg.freeTypeVars();
          AllFTVs.insert(FTV.begin(), FTV.end());
        }
        return AllFTVs;
      },
      [](const TypeApp &App) {
        std::unordered_set<TypeVar> AllFTVs;
        for (auto &Arg : App.Args) {
          auto FTV = Arg.freeTypeVars();
          AllFTVs.insert(FTV.begin(), FTV.end());
        }
        return AllFTVs;
      },
      [](const TypeFun &Fun) {
        std::unordered_set<TypeVar> AllFTVs;
        for (auto &Params : Fun.Params) {
          auto FTV = Params.freeTypeVars();
          AllFTVs.insert(FTV.begin(), FTV.end());
        }
        auto Ret = Fun.Ret->freeTypeVars();
        AllFTVs.insert(Ret.begin(), Ret.end());
        return AllFTVs;
      });
}

} // namespace phi
