#include "Sema/TypeInference/Infer.hpp"

#include <cassert>
#include <optional>
#include <print>

#include "AST/Decl.hpp"
#include "Sema/TypeInference/Types/Monotype.hpp"
#include "Sema/TypeInference/Types/Polytype.hpp"

namespace phi {

// ---------------- declarations ----------------
void TypeInferencer::visit(Decl &D) { D.accept(*this); }

void TypeInferencer::visit(VarDecl &D) {
  Monotype VarType = Monotype::makeVar(Factory.fresh());

  Substitution Subst;
  if (D.hasInit()) {
    auto [InitSubst, InitType] = visit(D.getInit());
    Subst = std::move(InitSubst);
    VarType = Subst.apply(VarType);
    if (VarType.isVar()) {
      auto varConstraints = VarType.asVar().Constraints;
      std::println("Vardecl {} is a var: {} constraints", D.getId(),
                   varConstraints ? varConstraints->size() : 0);
      if (InitType.isVar()) {
        auto initConstraints = InitType.asVar().Constraints;
        std::println("InitType has: {} constraints", 
                     initConstraints ? initConstraints->size() : 0);
      } else {
        std::println("InitType is concrete: {}", InitType.toString());
      }
    }

    unifyInto(Subst, VarType, InitType);
    // After unification, apply the substitution to get the unified result
    VarType = Subst.apply(VarType);
    InitType = Subst.apply(InitType);
  }

  if (D.hasType()) {
    const auto DeclaredAs = D.getType().toMonotype();
    unifyInto(Subst, VarType, DeclaredAs);
    VarType = Subst.apply(DeclaredAs);
  } else {
    VarType = Subst.apply(VarType);
  }

  // Propagate substitutions globally (so subsequent lookups see effects).
  recordSubst(Subst);

  // Local variable: bind a monomorphic scheme (no quantification).
  // This ensures the same Monotype instance stays associated with the VarDecl
  // for the remainder of the function, so later unifications update it.
  Env.bind(&D, Polytype{{}, VarType});

  // Side-table annotate (will be finalized later)
  annotate(D, VarType);
}

void TypeInferencer::visit(FunDecl &D) {
  std::optional<Monotype> FunType;

  // Lookup by name
  if (auto Polytype = Env.lookup(D.getId())) {
    FunType = Polytype->instantiate(Factory);
  }
  assert(FunType->isFun());

  // Save environment (we will restore)
  auto SavedEnv = Env;

  // Bind parameters (use user-declared types) for body inference
  for (auto &Param : D.getParams()) {
    assert(Param->hasType());

    auto T = Param->getType().toMonotype();
    Env.bind(Param.get(), Polytype{{}, T});
    // record param monotype too so we can finalize param annotations if desired
    ValDeclMonos[Param.get()] = T;
  }

  // Use declared return type as expected for body
  auto DeclaredRet = D.getReturnTy().toMonotype();
  CurFunRetType.push_back(DeclaredRet);

  // Infer body
  auto [BodySubst, _] = visit(D.getBody());

  // Propagate body substitution globally (important so call-sites see effects)
  recordSubst(BodySubst);

  // Apply substitution to function type
  FunType = BodySubst.apply(*FunType);

  CurFunRetType.pop_back();

  // Re-generalize in outer environment and rebind function name
  SavedEnv.bind(D.getId(), FunType->generalize(SavedEnv));

  FunDeclMonos[&D] = *FunType;

  // Restore outer environment
  Env = std::move(SavedEnv);
}

void TypeInferencer::visit(StructDecl &D) {
  // Create struct monotype and bind the struct name (so fields/methods can
  // reference it)
  Monotype StructMono = Monotype::makeVar(Factory.fresh());
  Env.bind(D.getId(), Polytype{{}, StructMono});

  // Bind fields into Env and annotate
  for (auto &Field : D.getFields()) {
    FieldDecl *TheDecl = Field.get();
    assert(TheDecl->hasType() && "struct fields must have type annotations");
    Monotype Type = TheDecl->getType().toMonotype();
    Env.bind(TheDecl, Polytype{{}, Type}); // <-- bind field decl into Env
    ValDeclMonos[TheDecl] = Type;
    annotate(*TheDecl, Type);
  }

  // Methods: same approach as functions but prepend self (StructMono).
  for (auto &Method : D.getMethods()) {
    MethodDecl *TheDecl = &Method;

    // Build method monotype
    std::vector<Monotype> ParamMonos;
    ParamMonos.reserve(1 + TheDecl->getParams().size());
    ParamMonos.push_back(StructMono); // self
    for (auto &Param : TheDecl->getParams())
      ParamMonos.push_back(Param->getType().toMonotype());
    Monotype RetMono = TheDecl->getReturnTy().toMonotype();
    Monotype MethodMono = Monotype::makeFun(std::move(ParamMonos), RetMono);

    auto SavedEnv = Env;

    for (auto &Param : TheDecl->getParams()) {
      Monotype ParamType = Param->getType().toMonotype();
      Env.bind(Param.get(), Polytype{{}, ParamType});
      ValDeclMonos[Param.get()] = ParamType;
    }

    CurFunRetType.push_back(RetMono);
    auto [BodySubst, _] = visit(TheDecl->getBody());
    recordSubst(BodySubst);
    MethodMono = BodySubst.apply(MethodMono);
    CurFunRetType.pop_back();

    // Unify declared param/return types with inferred
    auto &MethodMonotype = MethodMono.asFun();
    unifyInto(BodySubst, RetMono, *MethodMonotype.Ret);
    recordSubst(BodySubst);

    // Re-generalize and bind method under dotted name
    std::string Qualified = D.getId() + "." + TheDecl->getId();
    SavedEnv.bind(Qualified, MethodMono.generalize(SavedEnv));
    FunDeclMonos[TheDecl] = MethodMono;

    Env = std::move(SavedEnv);
  }
}

void TypeInferencer::visit(EnumDecl &D) {}

} // namespace phi
