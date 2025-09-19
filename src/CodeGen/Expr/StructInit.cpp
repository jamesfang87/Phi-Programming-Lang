#include "CodeGen/CodeGen.hpp"

#include <llvm/Support/Casting.h>

using namespace phi;

llvm::Value *CodeGen::visit(StructInitExpr &E) {}

llvm::Value *CodeGen::visit(FieldInitExpr &E) {}
