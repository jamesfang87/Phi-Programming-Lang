#include "AST/Type.hpp"

namespace phi {
/**
 * Checks if type is an integer primitive.
 */
bool isIntTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::I8Kind ||
         prim == Type::PrimitiveKind::I16Kind ||
         prim == Type::PrimitiveKind::I32Kind ||
         prim == Type::PrimitiveKind::I64Kind ||
         prim == Type::PrimitiveKind::U8Kind ||
         prim == Type::PrimitiveKind::U16Kind ||
         prim == Type::PrimitiveKind::U32Kind ||
         prim == Type::PrimitiveKind::U64Kind;
}

bool isSignedInt(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::I8Kind ||
         prim == Type::PrimitiveKind::I16Kind ||
         prim == Type::PrimitiveKind::I32Kind ||
         prim == Type::PrimitiveKind::I64Kind;
}

bool isUnsignedInt(const Type &Ty) { return (isIntTy(Ty) && !isSignedInt(Ty)); }

bool isFloat(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  const Type::PrimitiveKind prim = Ty.getPrimitiveType();
  return prim == Type::PrimitiveKind::F32Kind ||
         prim == Type::PrimitiveKind::F64Kind;
}

/**
 * Checks if type is numeric (integer or float).
 */
bool isNumTy(const Type &Ty) {
  if (!Ty.isPrimitive())
    return false;

  return isIntTy(Ty) || isFloat(Ty);
}

} // namespace phi
