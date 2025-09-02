#include "AST/Type.hpp"

#include <llvm/IR/DerivedTypes.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Type.h>

#include "Sema/TypeInference/Types/Monotype.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

std::string Type::toString() const {
  SrcLocation L = this->Location;
  struct V {
    SrcLocation L;
    std::string operator()(PrimitiveKind K) const {
      return std::format("{}", primitiveKindToString(K));
    }
    std::string operator()(const CustomType &C) const { return C.Name; }
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
          Oss << ",";
      }
      Oss << ">";
      return Oss.str();
    }
    std::string operator()(const FunctionType &F) const {
      std::ostringstream Oss;
      Oss << "fn(";
      for (size_t I = 0; I < F.Parameters.size(); ++I) {
        Oss << F.Parameters[I].toString();
        if (I + 1 < F.Parameters.size())
          Oss << ",";
      }
      Oss << ") -> " << F.ReturnType->toString();
      return Oss.str();
    }
  };
  return std::visit(V{L}, Data);
}

class Monotype Type::toMonotype() const {
  SrcLocation L = this->Location;
  struct Visitor {
    SrcLocation L;
    Monotype operator()(PrimitiveKind K) const {
      return Monotype::makeCon(primitiveKindToString(K), {}, L);
    }
    Monotype operator()(const CustomType &C) const {
      return Monotype::makeCon(C.Name, {}, L);
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
      Monotype ret = F.ReturnType->toMonotype();
      return Monotype::makeFun(Params, ret, L);
    }
  };
  return std::visit(Visitor{L}, Data);
}

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

    llvm::Type *operator()(const CustomType &C) const {
      // Reuse an existing named struct if it exists; otherwise create opaque.
      if (auto *T = llvm::StructType::getTypeByName(Ctx, C.Name))
        return T;
      return llvm::StructType::create(Ctx, C.Name);
    }

    llvm::Type *operator()(const ReferenceType &R) const {
      (void)R;
      return llvm::PointerType::getUnqual(Ctx);
    }

    llvm::Type *operator()(const PointerType &P) const {
      (void)P;
      return llvm::PointerType::getUnqual(Ctx);
    }

    llvm::Type *operator()(const GenericType &G) const {
      // Define a simple lowering policy. Adjust as your runtime dictates.

      // Vector<T> → { i64 len, T* data }
      if (G.Name == "Vector" && G.TypeArguments.size() == 1) {
        llvm::Type *ElemTy = G.TypeArguments[0].toLLVM(Ctx);
        auto *LenTy = llvm::Type::getInt64Ty(Ctx);
        return llvm::StructType::get(Ctx, {LenTy, ElemTy->getPointerTo()});
      }

      // Map<K,V> → opaque named struct "Map" (shared across instantiations).
      // If you need K/V-specific layouts, you’ll want a mangled unique name.
      if (G.Name == "Map" && G.TypeArguments.size() == 2) {
        if (auto *T = llvm::StructType::getTypeByName(Ctx, "Map"))
          return T;
        return llvm::StructType::create(Ctx, "Map");
      }

      // Fallback: opaque named struct with the generic’s base name.
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
