#include "CodeGen/CodeGen.hpp"

#include <cassert>
#include <llvm/IR/DerivedTypes.h>
#include <llvm/Support/Casting.h>

using namespace phi;

void CodeGen::visit(Decl &D) { D.accept(*this); }

void CodeGen::visit(VarDecl &D) {
  llvm::AllocaInst *Var = stackAlloca(D);

  if (D.hasInit())
    store(load(visit(D.getInit()), D.getType()), Var, D.getInit().getType());

  DeclMap[&D] = Var;
}
