#include "Sema/NameResolver.hpp"

#include <format>
#include <print>
#include <string>

#include "AST/Type.hpp"

namespace phi {

bool NameResolver::resolveTypeByName(const phi::Type &Type,
                                     const std::string &Name) {
  if (this->SymbolTab.lookupStruct(Name) || this->SymbolTab.lookupEnum(Name))
    return true;

  if (auto Best = this->SymbolTab.getClosestType(Name)) {
    std::string Hint = std::format("Did you mean `{}`?", *Best);
  }

  this->emitNotFoundError(NotFoundErrorKind::Type, Name, Type.getLocation());
  return false;
}

bool NameResolver::visit(std::optional<phi::Type> MaybeType) {
  if (!MaybeType)
    return false;

  // Make a local copy/reference so we can call toString() / getLocation()
  // later.
  phi::Type &T = *MaybeType;
  struct Visitor {
    NameResolver *Resolver;
    phi::Type &Cur; // the original phi::Type for toString() / location

    // Primitive -> accept
    bool operator()(phi::PrimitiveKind) const { return true; }

    // Pointer -> recurse into the pointee's variant
    bool operator()(const phi::PointerType &P) const {
      this->Cur = *P.Pointee;
      return std::visit(*this, P.Pointee->getData());
    }

    // Reference -> recurse into the pointee's variant
    bool operator()(const phi::ReferenceType &R) const {
      this->Cur = *R.Pointee;
      return std::visit(*this, R.Pointee->getData());
    }

    bool operator()(const phi::TupleType &T) const {
      bool Success = true;
      for (const auto &ElementTy : T.Types) {
        this->Cur = ElementTy;
        Success = std::visit(*this, ElementTy.getData()) && Success;
      }
      return Success;
    }

    bool operator()(const phi::StructType &S) const {
      (void)S;
      return Resolver->resolveTypeByName(Cur, Cur.toString());
    }

    bool operator()(const phi::EnumType &E) const {
      (void)E;
      return Resolver->resolveTypeByName(Cur, Cur.toString());
    }

    bool operator()(const phi::GenericType &G) const {
      (void)G;
      return Resolver->resolveTypeByName(Cur, Cur.toString());
    }

    bool operator()(const phi::FunctionType &F) const {
      (void)F;
      return Resolver->resolveTypeByName(Cur, Cur.toString());
    }
  };

  return std::visit(Visitor{.Resolver = this, .Cur = T}, T.getData());
}

} // namespace phi
