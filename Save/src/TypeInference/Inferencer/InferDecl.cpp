#include "Sema/TypeInference/Infer.hpp"

#include <cassert>
#include <optional>

#include "AST/Nodes/Stmt.hpp"
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

    unifyInto(Subst, VarType, InitType);
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

  recordSubst(Subst);
  Env.bind(&D, Polytype{{}, VarType});
  annotate(D, VarType);
}

void TypeInferencer::visit(ParamDecl &D) {
  assert(D.hasType());

  auto T = D.getType().toMonotype();
  Env.bind(&D, Polytype{{}, T});
  annotate(D, T);
}

void TypeInferencer::visit(FunDecl &D) {
  // 1) Lookup by name to get the Function's Type
  Monotype FunType = Env.lookup(D.getId())->instantiate(Factory);
  assert(FunType.isFun());

  // 2) Now we begin to infer the contents of the functions body
  auto OuterScopeEnv = Env; // we are about to enter the fun's scope

  // 3) Visit params
  for (auto &Param : D.getParams()) {
    visit(*Param);
  }

  // 4) Visit Body
  CurFunRetType.push_back(D.getReturnTy().toMonotype());
  auto [BodySubst, _] = visit(D.getBody());
  recordSubst(BodySubst);
  CurFunRetType.pop_back();

  // 5) Re-generalize in outer environment and rebind function name
  OuterScopeEnv.bind(D.getId(), FunType.generalize(OuterScopeEnv));
  Env = std::move(OuterScopeEnv);
}

void TypeInferencer::visit(FieldDecl &D) {
  assert(D.hasType() && "struct fields must have type annotations");

  Monotype Type = D.getType().toMonotype();
  Env.bind(&D, Polytype{{}, Type});
  annotate(D, Type);
}

void TypeInferencer::visit(MethodDecl &D) {
  // 1) Build method monotype
  auto AdtType = D.getParent()->getType().toMonotype();
  auto MethodMono = D.getType().toMonotype();

  // 2) Now we begin to infer the body of the method
  auto OuterScopeEnv = Env; // Save the outer scope

  // 3) Visit params
  for (auto &Param : D.getParams()) {
    visit(*Param);
  }

  // 4) Visit body
  CurFunRetType.push_back(D.getReturnTy().toMonotype());
  auto [BodySubst, _] = visit(D.getBody());
  recordSubst(BodySubst);
  CurFunRetType.pop_back();

  // 5) Re-generalize and bind method under dotted name
  std::string Qualified = D.getParent()->getId() + "." + D.getId();
  OuterScopeEnv.bind(Qualified, MethodMono.generalize(OuterScopeEnv));
  Env = std::move(OuterScopeEnv);
}

void TypeInferencer::visit(StructDecl &D) {
  // 1) Create struct monotype and bind the struct name
  Env.bind(D.getId(), Polytype{{}, D.getType().toMonotype()});

  // 2) Bind fields into Env and annotate
  for (auto &Field : D.getFields()) {
    visit(Field);
  }

  // 3) Infer types for methods
  for (auto &Method : D.getMethods()) {
    visit(Method);
  }
}

void TypeInferencer::visit(EnumDecl &D) {
  Env.bind(D.getId(), Polytype{{}, D.getType().toMonotype()});

  for (auto &Method : D.getMethods()) {
    visit(Method);
  }
}

} // namespace phi
