#include "AST/Type.hpp"

#include <cassert>
#include <sstream>
#include <string>

#include "llvm/IR/DerivedTypes.h"
#include "llvm/IR/LLVMContext.h"
#include "llvm/IR/Type.h"

#include "Sema/TypeInference/Types/Monotype.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

[[nodiscard]] const Type Type::getUnderlying() const {
  struct Visitor {
    const Type operator()(const PrimitiveKind &) const noexcept { return Self; }

    const Type operator()(const StructType &) const noexcept { return Self; }

    const Type operator()(const EnumType &) const noexcept { return Self; }

    const Type operator()(const TupleType &) const noexcept { return Self; }

    const Type operator()(const GenericType &) const noexcept { return Self; }

    const Type operator()(const FunctionType &) const noexcept { return Self; }

    const Type operator()(const ReferenceType &R) const noexcept {
      // Recurse until base type is not a pointer/ref
      return R.Pointee->getUnderlying();
    }

    const Type operator()(const PointerType &P) const noexcept {
      // Recurse until base type is not a pointer/ref
      return P.Pointee->getUnderlying();
    }

    const Type &Self;
  };

  return std::visit(Visitor{*this}, Data);
}

//===----------------------------------------------------------------------===//
// String Conversion Methods
//===----------------------------------------------------------------------===//

std::string Type::toString() const {
  SrcLocation L = this->Location;
  struct Visitor {
    SrcLocation L;

    std::string operator()(PrimitiveKind K) const {
      return std::format("{}", primitiveKindToString(K));
    }

    std::string operator()(const StructType &S) const { return S.Name; }

    std::string operator()(const EnumType &E) const { return E.Name; }

    std::string operator()(const TupleType &T) const {
      std::ostringstream Oss;
      Oss << "(";
      for (size_t I = 0; I < T.Types.size(); ++I) {
        Oss << T.Types[I].toString();
        if (I + 1 < T.Types.size())
          Oss << ", ";
      }
      Oss << ")";
      return Oss.str();
    }

    std::string operator()(const ReferenceType &R) const {
      return "&" + R.Pointee->toString();
    }

    std::string operator()(const PointerType &P) const {
      return "*" + P.Pointee->toString();
    }

    std::string operator()(const GenericType &G) const {
      std::ostringstream Oss;
      Oss << G.Name << "<";
      for (size_t I = 0; I < G.TypeArguments.size(); ++I) {
        Oss << G.TypeArguments[I].toString();
        if (I + 1 < G.TypeArguments.size())
          Oss << ", ";
      }
      Oss << ">";
      return Oss.str();
    }

    std::string operator()(const FunctionType &F) const {
      std::ostringstream Oss;
      Oss << "fun(";
      for (size_t I = 0; I < F.Parameters.size(); ++I) {
        Oss << F.Parameters[I].toString();
        if (I + 1 < F.Parameters.size())
          Oss << ", ";
      }
      Oss << ") -> " << F.ReturnType->toString();
      return Oss.str();
    }
  };
  return std::visit(Visitor{L}, Data);
}

//===----------------------------------------------------------------------===//
// Monotype Conversion Methods
//===----------------------------------------------------------------------===//

Monotype Type::toMonotype() const {
  const SrcLocation L = this->Location;
  struct Visitor {
    SrcLocation L;

    Monotype operator()(PrimitiveKind K) const {
      return Monotype::makeCon(primitiveKindToString(K), {}, L);
    }

    Monotype operator()(const StructType &S) const {
      return Monotype::makeCon(S.Name, {}, L);
    }

    Monotype operator()(const EnumType &C) const {
      return Monotype::makeCon(C.Name, {}, L);
    }

    Monotype operator()(const TupleType &T) const {
      // TODO: Create Type Constructor so that this is more pedantic
      std::vector<Monotype> Types;
      Types.reserve(T.Types.size());
      for (const auto &Arg : T.Types) {
        Types.push_back(Arg.toMonotype());
      }
      return Monotype::makeApp("Tuple", Types, L);
    }

    Monotype operator()(const ReferenceType &R) const {
      return Monotype::makeApp("Ref", {R.Pointee->toMonotype()}, L);
    }

    Monotype operator()(const PointerType &P) const {
      return Monotype::makeApp("Ptr", {P.Pointee->toMonotype()}, L);
    }

    Monotype operator()(const GenericType &G) const {
      std::vector<Monotype> Args;
      Args.reserve(G.TypeArguments.size());
      for (const auto &Arg : G.TypeArguments) {
        Args.push_back(Arg.toMonotype());
      }
      return Monotype::makeApp(G.Name, Args, L);
    }

    Monotype operator()(const FunctionType &F) const {
      std::vector<Monotype> Params;
      Params.reserve(F.Parameters.size());
      for (const auto &Param : F.Parameters)
        Params.push_back(Param.toMonotype());
      const Monotype Ret = F.ReturnType->toMonotype();
      return Monotype::makeFun(Params, Ret, L);
    }
  };
  return std::visit(Visitor{L}, Data);
}

//===----------------------------------------------------------------------===//
// LLVM Type Conversion Methods
//===----------------------------------------------------------------------===//

llvm::Type *Type::toLLVM(llvm::LLVMContext &Ctx) const {
  struct Visitor {
    llvm::LLVMContext &Ctx;

    llvm::Type *operator()(PrimitiveKind K) const {
      switch (K) {
      case PrimitiveKind::I8:
        return llvm::Type::getInt8Ty(Ctx);
      case PrimitiveKind::I16:
        return llvm::Type::getInt16Ty(Ctx);
      case PrimitiveKind::I32:
        return llvm::Type::getInt32Ty(Ctx);
      case PrimitiveKind::I64:
        return llvm::Type::getInt64Ty(Ctx);
      case PrimitiveKind::U8:
        return llvm::Type::getInt8Ty(Ctx);
      case PrimitiveKind::U16:
        return llvm::Type::getInt16Ty(Ctx);
      case PrimitiveKind::U32:
        return llvm::Type::getInt32Ty(Ctx);
      case PrimitiveKind::U64:
        return llvm::Type::getInt64Ty(Ctx);
      case PrimitiveKind::F32:
        return llvm::Type::getFloatTy(Ctx);
      case PrimitiveKind::F64:
        return llvm::Type::getDoubleTy(Ctx);
      case PrimitiveKind::Bool:
        return llvm::Type::getInt1Ty(Ctx);
      case PrimitiveKind::Char: // Unicode scalar value; change to i8 if ASCII
        return llvm::Type::getInt32Ty(Ctx);
      case PrimitiveKind::String:
        // No native string in LLVM → represent as i8*
        return llvm::PointerType::getUnqual(Ctx);
      case PrimitiveKind::Range: {
        auto *I64 = llvm::Type::getInt64Ty(Ctx);
        return llvm::StructType::get(Ctx, {I64, I64}); // {start, end}
      }
      case PrimitiveKind::Null:
        // Treat as 'void' (valid only as a function return type)
        return llvm::Type::getVoidTy(Ctx);
      }
      // Should be unreachable, but keep it compile-friendly:
      assert(false && "Unhandled PrimitiveKind");
      return llvm::Type::getVoidTy(Ctx);
    }

    llvm::Type *operator()(const StructType &C) const {
      // Reuse an existing named struct if it exists; otherwise create opaque.
      if (auto *T = llvm::StructType::getTypeByName(Ctx, C.Name))
        return T;
      return llvm::StructType::create(Ctx, C.Name);
    }

    llvm::Type *operator()(const EnumType &C) const {
      // Reuse an existing named struct if it exists; otherwise create opaque.
      if (auto *T = llvm::StructType::getTypeByName(Ctx, C.Name))
        return T;
      return llvm::StructType::create(Ctx, C.Name);
    }

    llvm::Type *operator()(const TupleType &T) const {
      // Empty tuple -> treat as unit. Choose either `void` or an empty struct.
      if (T.Types.empty()) {
        assert(false && "A Tuple should never be empty");
        return llvm::StructType::get(Ctx, {});
      }

      std::vector<llvm::Type *> Types;
      Types.reserve(T.Types.size());
      for (const auto &E : T.Types)
        Types.push_back(E.toLLVM(Ctx));

      // Create an anonymous (literal) struct to represent the tuple
      return llvm::StructType::get(Ctx, Types, /*isPacked=*/false);
    }

    llvm::Type *operator()(const ReferenceType &R) const {
      llvm::Type *PointeeTy = R.Pointee->toLLVM(Ctx);
      return llvm::PointerType::getUnqual(PointeeTy);
    }

    llvm::Type *operator()(const PointerType &P) const {
      llvm::Type *PointeeTy = P.Pointee->toLLVM(Ctx);
      return llvm::PointerType::getUnqual(PointeeTy);
    }

    llvm::Type *operator()(const GenericType &G) const {
      // Fallback: opaque named struct with the generic's base name.
      if (auto *T = llvm::StructType::getTypeByName(Ctx, G.Name))
        return T;
      return llvm::StructType::create(Ctx, G.Name);
    }

    llvm::Type *operator()(const FunctionType &F) const {
      std::vector<llvm::Type *> ParamTys;
      ParamTys.reserve(F.Parameters.size());
      for (const auto &P : F.Parameters)
        ParamTys.push_back(P.toLLVM(Ctx));

      llvm::Type *RetTy = F.ReturnType->toLLVM(Ctx);
      return llvm::FunctionType::get(RetTy, ParamTys, /*isVarArg=*/false);
    }
  };

  return std::visit(Visitor{Ctx}, Data);
}

} // namespace phi
