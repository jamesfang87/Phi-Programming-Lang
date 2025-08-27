#include "AST/Type.hpp"
#include "Sema/TypeInference/TypeEnv.hpp"

#include <cassert>
#include <print>
#include <string>
#include <vector>

namespace phi {

Type Monotype::toAstType() const {
  return this->visit(
      [&](const TypeVar &Var) {
        (void)Var;
        std::println("Cannot convert Monotype which has unknown type to a "
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
        std::println("Here");
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

} // namespace phi
