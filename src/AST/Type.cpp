#include "AST/Type.hpp"

#include "Sema/TypeInference/Types/Monotype.hpp"
#include "SrcManager/SrcLocation.hpp"

namespace phi {

class Monotype; // forward declare your Monotype class

Monotype Type::toMonotype() const {
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

} // namespace phi
