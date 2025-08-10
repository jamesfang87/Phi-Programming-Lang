#include "Sema/Sema.hpp"

#include "AST/Decl.hpp"

namespace phi {

bool Sema::resolveStructDecl(StructDecl *Struct) {
  if (!Struct) {
    std::println("failed to resolve declaration: {}", Struct->getId());
    return false;
  }

  for (auto &Field : Struct->getFields()) {
    if (!resolveFieldDecl(&Field, Struct)) {
      std::println("failed to resolve field: {}", Field.getId());
      return false;
    }
  }

  for (auto &Method : Struct->getMethods()) {
    if (!resolveFunDecl(&Method)) {
      std::println("failed to resolve method: {}", Method.getId());
      return false;
    }
  }

  return true;
}

bool Sema::resolveFieldDecl(FieldDecl *Field, StructDecl *Parent) {
  if (!resolveTy(Field->getType())) {
    std::println("invalid type for parameter: {}", Field->getId());
    return false;
  }

  // Struct holding itself will create infinite size
  if (Field->getType() == Parent->getType()) {
    std::println("Field cannot be the same as the parent struct, considering "
                 "using a ptr");
    return false;
  }

  // Handle initializer if present
  if (Field->hasInit()) {
    Expr &Init = Field->getInit();
    if (!Init.accept(*this)) {
      std::println("failed to resolve field initializer");
      return false;
    }

    // Check type compatibility
    if (Init.getType() != Field->getType()) {
      std::println("field initializer type mismatch");
      std::println("field type: {}", Field->getType().toString());
      std::println("initializer type: {}", Init.getType().toString());
      return false;
    }
  }

  return true;
}

} // namespace phi
